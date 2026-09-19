//! What an agent did in a session, from the agent's own log, as metadata.
//!
//! The owner binds a session to a log file in `<state-dir>/agents.json` (private, owner-only).
//! Nothing is read without that binding. From each log line this keeps: when, which tool, whether
//! it failed, a short fingerprint of its input (to spot the same call being repeated), and
//! compaction sizes. It never returns prompts, reasoning, command text, file paths or results.
use anyhow::{Context, Result, bail};
use ring::digest::{SHA256, digest};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Seek, SeekFrom},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

const MAX_SCAN: u64 = 8 * 1024 * 1024; // per request
const MAX_EVENTS: usize = 2000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    kind: String,
    path: PathBuf,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    schema: String,
    sessions: HashMap<String, Binding>,
}

fn private_file(path: &Path) -> Result<()> {
    let m = std::fs::symlink_metadata(path)?;
    if !m.is_file() || m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o077 != 0 {
        bail!("must be a private file owned by this user")
    }
    Ok(())
}

/// The log bound to `session`, if the owner bound one.
pub fn bound(dir: &Path, session: &str) -> Result<Option<PathBuf>> {
    let path = dir.join("agents.json");
    if std::fs::symlink_metadata(&path).is_err() {
        return Ok(None);
    }
    private_file(&path).context("agents.json")?;
    let file: File = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    if file.schema != "dot.agents.v1" {
        bail!("unsupported agents file")
    }
    let Some(b) = file.sessions.get(session) else {
        return Ok(None);
    };
    if b.kind != "claude-jsonl" || !b.path.is_absolute() {
        bail!("unsupported agent log binding")
    }
    // The log itself must be the owner's; it may be group/other-unreadable or not, that is theirs.
    let m = std::fs::metadata(&b.path)?;
    if !m.is_file() || m.uid() != unsafe { libc::geteuid() } {
        bail!("agent log must be a file owned by this user")
    }
    Ok(Some(b.path.clone()))
}

fn category(tool: &str) -> &'static str {
    match tool {
        "Bash" | "BashOutput" | "KillShell" => "shell",
        "Read" | "Edit" | "Write" | "Glob" | "Grep" | "NotebookEdit" | "MultiEdit" => "files",
        "WebFetch" | "WebSearch" => "browser",
        "Agent" | "Task" | "Workflow" | "SendMessage" => "agent",
        "TodoWrite" | "TaskCreate" | "TaskUpdate" | "TaskList" => "tasks",
        t if t.starts_with("mcp__claude-in-chrome") => "browser",
        t if t.contains("oracle") => "oracle",
        _ => "other",
    }
}
fn tool_name(raw: &str) -> String {
    raw.chars()
        .filter(|c| c.is_ascii_alphanumeric() || "_.:-".contains(*c))
        .take(48)
        .collect()
}
fn id_ok(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// One log line → zero or more metadata events.
pub fn reduce(line: &str, out: &mut Vec<Value>) {
    let Ok(r) = serde_json::from_str::<Value>(line) else {
        return;
    };
    let Some(at) = r["timestamp"].as_str().filter(|t| t.len() <= 40) else {
        return;
    };
    if r["subtype"] == "compact_boundary" {
        let m = &r["compactMetadata"];
        out.push(json!({"kind":"compact","at":at,"before":m["preTokens"].as_u64(),"after":m["postTokens"].as_u64(),"durationMs":m["durationMs"].as_u64()}));
        return;
    }
    let role = r["message"]["role"].as_str().unwrap_or("");
    let content = &r["message"]["content"];
    if role == "user" && content.is_string() && r["isMeta"] != true {
        out.push(json!({"kind":"input","at":at}));
        return;
    }
    let mut typed = false;
    for item in content.as_array().into_iter().flatten() {
        match item["type"].as_str() {
            Some("tool_use") if role == "assistant" => {
                let (Some(id), Some(name)) = (item["id"].as_str(), item["name"].as_str()) else {
                    continue;
                };
                if !id_ok(id) {
                    continue;
                }
                // A fingerprint of tool + input: equal fingerprints are the same call again.
                let mut material = name.as_bytes().to_vec();
                material.extend_from_slice(item["input"].to_string().as_bytes());
                let sig = hex::encode(&digest(&SHA256, &material).as_ref()[..6]);
                out.push(json!({"kind":"tool","phase":"start","at":at,"id":id,"tool":tool_name(name),"category":category(name),"sig":sig}));
            }
            Some("tool_result") if role == "user" => {
                let Some(id) = item["tool_use_id"].as_str().filter(|i| id_ok(i)) else {
                    continue;
                };
                out.push(json!({"kind":"tool","phase":"end","at":at,"id":id,"error":item["is_error"]==true}));
            }
            Some("text") if role == "user" && r["isMeta"] != true => typed = true,
            _ => {}
        }
    }
    if typed {
        out.push(json!({"kind":"input","at":at}));
    }
}

/// Where to start so that roughly the last `tail` bytes are read: the first line start at or after
/// `size - tail`. A long-running agent's log can be hundreds of MB; a view wants the recent part.
pub fn tail_start(path: &Path, tail: u64) -> Result<u64> {
    let mut file = std::fs::File::open(path)?;
    let size = file.metadata()?.len();
    if size <= tail {
        return Ok(0);
    }
    let from = size - tail;
    file.seek(SeekFrom::Start(from))?;
    let mut skipped = Vec::new();
    let n = BufReader::new(file).read_until(b'\n', &mut skipped)? as u64;
    Ok((from + n).min(size))
}

/// Events from byte `after`. Only whole lines are consumed, so `next` is always a line start.
pub fn read(path: &Path, after: u64) -> Result<(Vec<Value>, u64, u64)> {
    let mut file = std::fs::File::open(path)?;
    let size = file.metadata()?.len();
    let mut at = after.min(size);
    file.seek(SeekFrom::Start(at))?;
    let mut reader = BufReader::new(file);
    let (mut events, mut line, start) = (vec![], Vec::new(), at);
    while at - start < MAX_SCAN && events.len() < MAX_EVENTS {
        line.clear();
        let n = reader.read_until(b'\n', &mut line)? as u64;
        if n == 0 || line.last() != Some(&b'\n') {
            break; // end of file, or a line still being written
        }
        at += n;
        if let Ok(text) = std::str::from_utf8(&line) {
            reduce(text, &mut events);
        }
    }
    Ok((events, at, size))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    const USE: &str = r#"{"timestamp":"2026-09-19T10:00:00Z","message":{"role":"assistant","content":[{"type":"text","text":"SECRET REASONING"},{"type":"tool_use","id":"toolu_01","name":"Bash","input":{"command":"cat ~/.ssh/id_rsa"}}]}}"#;
    const RESULT: &str = r#"{"timestamp":"2026-09-19T10:00:02Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_01","is_error":true,"content":"PRIVATE OUTPUT"}]}}"#;
    #[test]
    fn only_metadata_leaves_the_log() {
        let mut out = vec![];
        reduce(USE, &mut out);
        reduce(RESULT, &mut out);
        reduce(
            r#"{"timestamp":"2026-09-19T10:01:00Z","message":{"role":"user","content":"please deploy PASSWORD=hunter2"}}"#,
            &mut out,
        );
        let text = serde_json::to_string(&out).unwrap();
        for leaked in ["SECRET", "id_rsa", "PRIVATE", "hunter2", "deploy"] {
            assert!(!text.contains(leaked), "{leaked} leaked: {text}");
        }
        assert_eq!(out.len(), 3);
        assert_eq!(
            (out[0]["tool"].as_str(), out[0]["category"].as_str()),
            (Some("Bash"), Some("shell"))
        );
        assert_eq!(out[1]["error"], true);
        assert_eq!(out[2]["kind"], "input");
    }
    #[test]
    fn the_same_call_has_the_same_fingerprint_and_a_different_one_does_not() {
        let mut out = vec![];
        reduce(USE, &mut out);
        reduce(USE, &mut out);
        reduce(&USE.replace("id_rsa", "known_hosts"), &mut out);
        assert_eq!(out[0]["sig"], out[1]["sig"]);
        assert_ne!(out[0]["sig"], out[2]["sig"]);
        assert_eq!(out[0]["sig"].as_str().unwrap().len(), 12);
    }
    #[test]
    fn hostile_names_ids_and_junk_lines_are_contained() {
        let mut out = vec![];
        reduce("not json", &mut out);
        reduce(r#"{"message":{"role":"assistant","content":[]}}"#, &mut out);
        reduce(
            r#"{"timestamp":"t","message":{"role":"assistant","content":[{"type":"tool_use","id":"<script>","name":"x","input":{}}]}}"#,
            &mut out,
        );
        assert!(out.is_empty());
        reduce(
            r#"{"timestamp":"t","message":{"role":"assistant","content":[{"type":"tool_use","id":"ok_1","name":"<b>Bash</b> rm -rf","input":{}}]}}"#,
            &mut out,
        );
        assert_eq!(out[0]["tool"], "bBashbrm-rf");
        assert_eq!(out[0]["category"], "other");
    }
    #[test]
    fn reading_resumes_on_line_boundaries_and_ignores_a_half_written_line() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("s.jsonl");
        std::fs::write(&log, format!("{USE}\n{RESULT}\n{{\"timestamp\":\"half")).unwrap();
        let (events, next, size) = read(&log, 0).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(next as usize, USE.len() + RESULT.len() + 2);
        assert!(size > next);
        let (again, same, _) = read(&log, next).unwrap();
        assert!(again.is_empty() && same == next);
    }
    #[test]
    fn a_tail_read_starts_on_a_line_boundary() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("s.jsonl");
        std::fs::write(&log, format!("{USE}\n{RESULT}\n")).unwrap();
        assert_eq!(
            tail_start(&log, 1 << 30).unwrap(),
            0,
            "small logs are read whole"
        );
        let start = tail_start(&log, RESULT.len() as u64 + 10).unwrap();
        assert_eq!(
            start as usize,
            USE.len() + 1,
            "skips the partial first line"
        );
        let (events, _, _) = read(&log, start).unwrap();
        assert_eq!(events.len(), 1);
    }
    #[test]
    fn nothing_is_read_without_a_private_owner_binding() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("s.jsonl");
        std::fs::write(&log, "").unwrap();
        assert!(
            bound(dir.path(), "abc").unwrap().is_none(),
            "no agents.json: nothing bound"
        );
        let file = dir.path().join("agents.json");
        let body = format!(
            r#"{{"schema":"dot.agents.v1","sessions":{{"abc":{{"kind":"claude-jsonl","path":"{}"}}}}}}"#,
            log.display()
        );
        std::fs::write(&file, &body).unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(
            bound(dir.path(), "abc").is_err(),
            "a shared agents.json is refused"
        );
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(bound(dir.path(), "abc").unwrap(), Some(log));
        assert!(bound(dir.path(), "other").unwrap().is_none());
        std::fs::write(&file, body.replace("claude-jsonl", "anything")).unwrap();
        assert!(bound(dir.path(), "abc").is_err());
    }
}
