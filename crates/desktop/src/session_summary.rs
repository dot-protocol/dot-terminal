//! Minimal shared session metadata and sampled descendant usage. No terminal content.
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::HashSet, path::Path};
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Label {
    pub device: String,
    pub id: String,
    pub name: String,
}
impl Label {
    fn valid(&self) -> bool {
        [&self.device, &self.id].iter().all(|s| {
            !s.is_empty()
                && s.len() <= 64
                && s.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        }) && (if self.device.starts_with("external-") {
            self.id
                .strip_prefix("s_")
                .is_some_and(|id| id.len() == 32 && id.bytes().all(|c| c.is_ascii_hexdigit()))
        } else {
            self.id.len() == 32 && self.id.bytes().all(|c| c.is_ascii_hexdigit())
        }) && !self.name.trim().is_empty()
            && self.name.len() <= 160
            && !self.name.chars().any(char::is_control)
    }
    pub fn save(mut self, root: &Path) -> Result<()> {
        if !self.valid() {
            bail!("invalid session label");
        }
        self.name = self.name.trim().to_owned();
        let dir = root.join("session-labels");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}-{}.json", self.device, self.id));
        // Unique temporary file permits concurrent views without truncating each other's writes.
        let mut tmp = tempfile::NamedTempFile::new_in(&dir)?;
        serde_json::to_writer(&mut tmp, &self)?;
        tmp.as_file().sync_all()?;
        tmp.persist(path)?;
        Ok(())
    }
}
pub fn labels(root: &Path) -> Value {
    let mut out = serde_json::Map::new();
    if let Ok(entries) = std::fs::read_dir(root.join("session-labels")) {
        for entry in entries.flatten().take(6400) {
            if let Ok(bytes) = std::fs::read(entry.path())
                && let Ok(label) = serde_json::from_slice::<Label>(&bytes)
                && label.valid()
            {
                out.insert(format!("{}/{}", label.device, label.id), json!(label.name));
            }
        }
    }
    Value::Object(out)
}
pub fn usage(snapshot: &Value, pid: Option<u64>) -> Value {
    let Some(pid) = pid else {
        return json!({"state":"unknown"});
    };
    let Some(processes) = snapshot["processes"].as_array() else {
        return json!({"state":"unknown"});
    };
    if !processes.iter().any(|p| p["pid"].as_u64() == Some(pid)) {
        return json!({"state":"unknown"});
    }
    let mut ids = HashSet::from([pid]);
    loop {
        let before = ids.len();
        for p in processes {
            if p["ppid"].as_u64().is_some_and(|p| ids.contains(&p))
                && let Some(p) = p["pid"].as_u64()
            {
                ids.insert(p);
            }
        }
        if before == ids.len() {
            break;
        }
    }
    let mut cpu = 0.0;
    let mut resident = 0u64;
    for p in processes
        .iter()
        .filter(|p| p["pid"].as_u64().is_some_and(|p| ids.contains(&p)))
    {
        cpu += p["cpu"].as_f64().unwrap_or(0.0);
        resident = resident.saturating_add(p["resident"].as_u64().unwrap_or(0));
    }
    json!({"state":"sampled", "at":snapshot["at"], "cpu":cpu, "resident":resident, "processes":ids.len()})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn external_labels_preserve_safe_namespace() {
        let mut label = Label {
            device: "external-server".into(),
            id: "s_0123456789abcdef0123456789abcdef".into(),
            name: "Research".into(),
        };
        assert!(label.valid());
        label.device = "local".into();
        assert!(!label.valid());
        label.device = "external-server".into();
        label.id = "s_../../private".into();
        assert!(!label.valid());
    }
    #[test]
    fn labels_persist_and_reject_paths() {
        let dir = tempfile::tempdir().unwrap();
        Label {
            device: "local".into(),
            id: "abcdef0123456789abcdef0123456789".into(),
            name: "Research".into(),
        }
        .save(dir.path())
        .unwrap();
        assert_eq!(
            labels(dir.path())["local/abcdef0123456789abcdef0123456789"],
            "Research"
        );
        assert!(
            Label {
                device: "../escape".into(),
                id: "abcdef0123456789abcdef0123456789".into(),
                name: "bad".into()
            }
            .save(dir.path())
            .is_err()
        );
    }
    #[test]
    fn counts_descendants_not_siblings() {
        let s = json!({"at":123,"processes":[{"pid":1,"cpu":1.,"resident":10},{"pid":2,"ppid":1,"cpu":5.,"resident":20},{"pid":3,"ppid":2,"cpu":10.,"resident":30},{"pid":4,"cpu":80.,"resident":400}]});
        assert_eq!(usage(&s, Some(1))["resident"], 60);
        assert_eq!(usage(&s, Some(1))["cpu"], 16.);
        assert_eq!(usage(&s, Some(9))["state"], "unknown");
    }
}
