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
fn apply(root: &Path, m: Mutation) -> anyhow::Result<Snapshot> {
    let tab = Tab {
        device: m.device,
        id: m.id,
    };
    anyhow::ensure!(tab.valid(), "invalid tab reference");
    let _guard = WRITE
        .lock()
        .map_err(|_| anyhow::anyhow!("tab state unavailable"))?;
    let mut s = read(root)?;
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
    let mut tmp = tempfile::NamedTempFile::new_in(root)?;
    serde_json::to_writer(&mut tmp, &s)?;
    tmp.as_file().sync_all()?;
    tmp.persist(root.join("workspace-tabs.json"))?;
    Ok(s)
}
pub async fn get(State(app): State<Shared>) -> Api {
    Ok(Json(
        serde_json::to_value(read(&app.dir).map_err(failed)?).map_err(failed)?,
    ))
}
pub async fn mutate(State(app): State<Shared>, Json(m): Json<Mutation>) -> Api {
    Ok(Json(
        serde_json::to_value(apply(&app.dir, m).map_err(failed)?).map_err(failed)?,
    ))
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
    fn rejects_traversal_and_corrupt_state_without_overwriting() {
        let dir = tempfile::tempdir().unwrap();
        let mut m = mutation(Action::Open, 1);
        m.device = "../private".into();
        assert!(apply(dir.path(), m).is_err());
        std::fs::write(dir.path().join("workspace-tabs.json"), "broken").unwrap();
        assert!(apply(dir.path(), mutation(Action::Open, 1)).is_err());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("workspace-tabs.json")).unwrap(),
            "broken"
        );
    }
}
