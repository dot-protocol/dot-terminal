//! Shared workspace membership. A tab is a view reference, never a process lifetime.
use crate::{Api, Shared, failed};
use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::Mutex};
static WRITE: Mutex<()> = Mutex::new(());
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Tab {
    device: String,
    id: String,
}
impl Tab {
    fn valid(&self) -> bool {
        !self.device.is_empty()
            && self.device.len() <= 32
            && self
                .device
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            && if self.device.starts_with("external-") {
                self.id.strip_prefix("s_").is_some_and(hex_id)
            } else {
                hex_id(&self.id)
            }
    }
}
fn hex_id(s: &str) -> bool {
    s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit())
}
#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    revision: u64,
    tabs: Vec<Tab>,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Action {
    Open,
    Close,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mutation {
    action: Action,
    device: String,
    id: String,
}
fn read(root: &Path) -> anyhow::Result<Snapshot> {
    let path = root.join("workspace-tabs.json");
    match std::fs::read(path) {
        Ok(bytes) => {
            anyhow::ensure!(bytes.len() <= 32768, "tab state too large");
            let s: Snapshot = serde_json::from_slice(&bytes)?;
            anyhow::ensure!(
                s.tabs.len() <= 128 && s.tabs.iter().all(Tab::valid),
                "invalid tab state"
            );
            Ok(s)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Snapshot::default()),
        Err(e) => Err(e.into()),
    }
}
/// Saved tab state, holding the write lock. Bytes that are not valid tab state are moved aside as
/// `workspace-tabs.corrupt-<ms>.json` (never deleted, never overwritten) and the workspace starts
/// empty at a revision above any earlier one, so every view adopts it. An I/O error stays an error.
fn load(root: &Path) -> anyhow::Result<Snapshot> {
    match read(root) {
        Ok(s) => Ok(s),
        Err(e) if e.downcast_ref::<std::io::Error>().is_none() => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(1);
            std::fs::rename(
                root.join("workspace-tabs.json"),
                root.join(format!("workspace-tabs.corrupt-{now}.json")),
            )?;
            let fresh = Snapshot {
                revision: now,
                tabs: vec![],
            };
            persist(root, &fresh)?;
            Ok(fresh)
        }
        Err(e) => Err(e),
    }
}
fn persist(root: &Path, s: &Snapshot) -> anyhow::Result<()> {
    let mut tmp = tempfile::NamedTempFile::new_in(root)?;
    serde_json::to_writer(&mut tmp, s)?;
    tmp.as_file().sync_all()?;
    tmp.persist(root.join("workspace-tabs.json"))?;
    // The rename is durable only once the directory entry is.
    std::fs::File::open(root)?.sync_all()?;
    Ok(())
}
/// Unreadable tab states kept aside, newest first, until a person removes them.
fn quarantined(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.starts_with("workspace-tabs.corrupt-"))
        .collect();
    names.sort_unstable_by(|a, b| b.cmp(a));
    names.truncate(5);
    names
}
fn answer(root: &Path, s: Snapshot) -> anyhow::Result<serde_json::Value> {
    let mut v = serde_json::to_value(s)?;
    let kept = quarantined(root);
    if !kept.is_empty() {
        v["quarantined"] = serde_json::json!(kept);
    }
    Ok(v)
}
fn apply(root: &Path, m: Mutation) -> anyhow::Result<Snapshot> {
    let tab = Tab {
        device: m.device,
        id: m.id,
    };
    anyhow::ensure!(tab.valid(), "invalid tab reference");
    let _guard = WRITE
        .lock()
        .map_err(|_| anyhow::anyhow!("tab state unavailable"))?;
    let mut s = load(root)?;
    let present = s.tabs.contains(&tab);
    match m.action {
        Action::Open if !present => {
            anyhow::ensure!(s.tabs.len() < 128, "workspace tab limit");
            s.tabs.push(tab);
        }
        Action::Close if present => s.tabs.retain(|t| t != &tab),
        _ => return Ok(s),
    }
    s.revision = s
        .revision
        .checked_add(1)
        .ok_or_else(|| anyhow::anyhow!("revision exhausted"))?;
    persist(root, &s)?;
    Ok(s)
}
// File I/O and fsync run on the blocking pool, never on the async runtime's workers.
pub async fn get(State(app): State<Shared>) -> Api {
    let root = app.dir.clone();
    let v = tokio::task::spawn_blocking(move || {
        let _guard = WRITE
            .lock()
            .map_err(|_| anyhow::anyhow!("tab state unavailable"))?;
        let s = load(&root)?;
        answer(&root, s)
    })
    .await
    .map_err(failed)?
    .map_err(failed)?;
    Ok(Json(v))
}
pub async fn mutate(State(app): State<Shared>, Json(m): Json<Mutation>) -> Api {
    let root = app.dir.clone();
    let v = tokio::task::spawn_blocking(move || {
        let s = apply(&root, m)?;
        answer(&root, s)
    })
    .await
    .map_err(failed)?
    .map_err(failed)?;
    Ok(Json(v))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn mutation(action: Action, n: usize) -> Mutation {
        Mutation {
            action,
            device: "local".into(),
            id: format!("{n:032x}"),
        }
    }
    #[test]
    fn concurrent_tabs_survive_and_close_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        std::thread::scope(|scope| {
            for i in 0..16 {
                let root = dir.path();
                scope.spawn(move || {
                    apply(root, mutation(Action::Open, i)).unwrap();
                });
            }
        });
        assert_eq!(read(dir.path()).unwrap().tabs.len(), 16);
        let a = apply(dir.path(), mutation(Action::Close, 3)).unwrap();
        let b = apply(dir.path(), mutation(Action::Close, 3)).unwrap();
        assert_eq!(a.revision, b.revision);
        assert_eq!(b.tabs.len(), 15);
    }
    #[test]
    fn rejects_traversal() {
        let dir = tempfile::tempdir().unwrap();
        let mut m = mutation(Action::Open, 1);
        m.device = "../private".into();
        assert!(apply(dir.path(), m).is_err());
    }
    #[test]
    fn corrupt_state_is_kept_aside_and_the_workspace_recovers_at_a_higher_revision() {
        let dir = tempfile::tempdir().unwrap();
        let before = apply(dir.path(), mutation(Action::Open, 7))
            .unwrap()
            .revision;
        std::fs::write(dir.path().join("workspace-tabs.json"), "broken").unwrap();
        let after = apply(dir.path(), mutation(Action::Open, 1)).unwrap();
        assert_eq!(after.tabs.len(), 1, "tabs work again after recovery");
        assert!(
            after.revision > before,
            "views would ignore a recovered state at a lower revision"
        );
        let kept = quarantined(dir.path());
        assert_eq!(kept.len(), 1);
        assert_eq!(
            std::fs::read_to_string(dir.path().join(&kept[0])).unwrap(),
            "broken",
            "the unreadable bytes were not kept as they were"
        );
        let shown = answer(dir.path(), read(dir.path()).unwrap()).unwrap();
        assert_eq!(shown["quarantined"][0], kept[0].as_str());
    }
}
