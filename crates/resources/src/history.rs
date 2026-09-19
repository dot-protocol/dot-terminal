use crate::model::{Point, Snapshot};
use rusqlite::{params, Connection};
use std::path::Path;

pub struct History(Connection);
impl History {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let c = Connection::open(path)?;
        c.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA busy_timeout=1000; PRAGMA journal_size_limit=1048576;
          CREATE TABLE IF NOT EXISTS samples(at INTEGER PRIMARY KEY,cpu REAL NOT NULL,memory INTEGER NOT NULL,swap INTEGER NOT NULL);
          CREATE TABLE IF NOT EXISTS groups(at INTEGER NOT NULL,id TEXT NOT NULL,footprint INTEGER NOT NULL,cpu REAL NOT NULL,measured INTEGER NOT NULL,count INTEGER NOT NULL,PRIMARY KEY(at,id));
          PRAGMA user_version=1;")?;
        Ok(Self(c))
    }
    pub fn record(&mut self, s: &Snapshot) -> rusqlite::Result<()> {
        let tx = self.0.transaction()?;
        tx.execute(
            "INSERT OR REPLACE INTO samples VALUES (?1,?2,?3,?4)",
            params![s.at, s.cpu, s.memory_used, s.swap_used],
        )?;
        // Bounded one-minute history: at most 1440 samples and 50 groups per sample.
        for g in s.groups.iter().take(50) {
            tx.execute(
                "INSERT OR REPLACE INTO groups VALUES (?1,?2,?3,?4,?5,?6)",
                params![s.at, g.id, g.footprint, g.cpu, g.measured, g.count],
            )?;
        }
        let cutoff = s.at.saturating_sub(86400);
        tx.execute("DELETE FROM samples WHERE at <= ?1", [cutoff])?;
        tx.execute("DELETE FROM groups WHERE at <= ?1", [cutoff])?;
        // Also cap cardinality if wall time moves backwards or callers sample faster.
        tx.execute("DELETE FROM samples WHERE at NOT IN (SELECT at FROM samples ORDER BY at DESC LIMIT 1440)",[])?;
        tx.execute(
            "DELETE FROM groups WHERE at NOT IN (SELECT at FROM samples)",
            [],
        )?;
        tx.commit()
    }
    pub fn points(&self) -> rusqlite::Result<Vec<Point>> {
        let mut stmt = self
            .0
            .prepare("SELECT at,cpu,memory,swap FROM samples ORDER BY at")?;
        let rows = stmt.query_map([], |r| {
            Ok(Point {
                at: r.get(0)?,
                cpu: r.get(1)?,
                memory: r.get(2)?,
                swap: r.get(3)?,
            })
        })?;
        rows.collect()
    }
    pub fn group_points(&self, id: &str) -> rusqlite::Result<Vec<Point>> {
        let mut stmt = self.0.prepare(
            "SELECT at,cpu,footprint FROM groups WHERE id=?1 AND measured=count ORDER BY at",
        )?;
        let rows = stmt.query_map([id], |r| {
            Ok(Point {
                at: r.get(0)?,
                cpu: r.get(1)?,
                memory: r.get(2)?,
                swap: 0,
            })
        })?;
        rows.collect()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn database_survives_reopen_and_prunes_old_samples() {
        let path = std::env::temp_dir().join(format!(
            "resource-history-test-{}.sqlite",
            std::process::id()
        ));
        {
            let h = History::open(&path).unwrap();
            h.0.execute("INSERT INTO samples VALUES (1,2.5,100,20)", [])
                .unwrap();
        }
        let mut h = History::open(&path).unwrap();
        assert_eq!(h.points().unwrap()[0].memory, 100);
        let s = Snapshot {
            schema: 1,
            at: 90000,
            state: "alive".into(),
            issues: vec![],
            host: "test".into(),
            cores: 1,
            cpu: 1.,
            memory_total: 100,
            memory_used: 20,
            swap_used: 0,
            uptime: 0,
            collector_pid: 0,
            duration_ms: 0,
            attribution_at: None,
            processes: vec![],
            groups: vec![],
            services: vec![],
            history: vec![],
        };
        h.record(&s).unwrap();
        assert_eq!(h.points().unwrap().len(), 1);
        assert_eq!(h.points().unwrap()[0].at, 90000);
        drop(h);
        let _ = std::fs::remove_file(path);
    }
}
