use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Process {
    pub id: String,
    pub pid: u32,
    pub ppid: Option<u32>,
    pub started: u64,
    pub name: String,
    pub executable: Option<String>,
    pub cwd: Option<String>,
    pub cpu: f64,
    pub resident: u64,
    pub footprint: Option<u64>,
    pub group_id: String,
    pub group_name: String,
    pub kind: String,
    pub evidence: String,
    pub service: Option<String>,
    pub ports: Vec<String>,
    pub read_bytes: u64,
    pub written_bytes: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub count: usize,
    pub cpu: f64,
    pub footprint: u64,
    pub measured: usize,
    pub resident: u64,
    pub ports: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Point {
    pub at: u64,
    pub cpu: f64,
    pub memory: u64,
    pub swap: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Service {
    pub label: String,
    pub pid: Option<u32>,
    pub last_exit: i32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub schema: u32,
    pub at: u64,
    pub state: String,
    pub issues: Vec<String>,
    pub host: String,
    pub cores: usize,
    pub cpu: f64,
    pub memory_total: u64,
    pub memory_used: u64,
    pub swap_used: u64,
    pub uptime: u64,
    pub collector_pid: u32,
    pub duration_ms: u64,
    pub attribution_at: Option<u64>,
    pub processes: Vec<Process>,
    pub groups: Vec<Group>,
    pub services: Vec<Service>,
    pub history: Vec<Point>,
}

pub fn aggregate(processes: &[Process]) -> Vec<Group> {
    let mut groups: BTreeMap<String, Group> = BTreeMap::new();
    for p in processes {
        let g = groups.entry(p.group_id.clone()).or_insert_with(|| Group {
            id: p.group_id.clone(),
            name: p.group_name.clone(),
            kind: p.kind.clone(),
            count: 0,
            cpu: 0.0,
            footprint: 0,
            measured: 0,
            resident: 0,
            ports: vec![],
        });
        g.count += 1;
        g.cpu += p.cpu;
        g.resident += p.resident;
        if let Some(n) = p.footprint {
            g.footprint += n;
            g.measured += 1;
        }
        g.ports.extend(p.ports.iter().cloned());
    }
    let mut out: Vec<_> = groups.into_values().collect();
    for g in &mut out {
        g.ports.sort();
        g.ports.dedup();
    }
    out.sort_by(|a, b| b.footprint.cmp(&a.footprint).then(a.id.cmp(&b.id)));
    out
}

/// A path is evidence of location, not proof of inactivity or ownership.
pub fn classify(
    executable: Option<&str>,
    project: Option<&str>,
    cwd: Option<&str>,
    name: &str,
) -> (String, String, String, String) {
    if let Some(exe) = executable {
        if let Some(i) = exe.find(".app/") {
            let bundle = &exe[..i + 4];
            let label = bundle
                .rsplit('/')
                .next()
                .unwrap_or(bundle)
                .trim_end_matches(".app");
            return (
                format!("app:{bundle}"),
                label.into(),
                "Application".into(),
                format!("Executable belongs to {bundle}"),
            );
        }
    }
    if let Some(root) = project {
        let label = root.rsplit('/').next().unwrap_or(root);
        return (
            format!("project:{root}"),
            label.into(),
            "Project".into(),
            format!("Working directory is inside Git project {root}"),
        );
    }
    if let Some(dir) = cwd.filter(|d| *d != "/" && *d != "/var/empty") {
        if dir.starts_with("/Users/") && dir.split('/').count() > 4 {
            return (
                format!("directory:{dir}"),
                dir.rsplit('/').next().unwrap_or(dir).into(),
                "Directory".into(),
                format!("Shared working directory {dir}; project ownership unconfirmed"),
            );
        }
    }
    let key = executable.unwrap_or(name);
    (
        format!("executable:{key}"),
        name.into(),
        "Process".into(),
        "Grouped by executable only; ownership unconfirmed".into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_helpers_belong_to_outer_application() {
        let (id, name, kind, _) = classify(
            Some("/Applications/Chrome.app/Contents/Helpers/Renderer.app/Contents/MacOS/Renderer"),
            None,
            None,
            "Renderer",
        );
        assert_eq!(id, "app:/Applications/Chrome.app");
        assert_eq!(name, "Chrome");
        assert_eq!(kind, "Application");
    }
    #[test]
    fn same_named_projects_have_distinct_identity() {
        assert_ne!(
            classify(None, Some("/Users/a/one/server"), None, "node").0,
            classify(None, Some("/Users/a/two/server"), None, "node").0
        );
    }
    #[test]
    fn directory_is_not_claimed_to_be_a_project() {
        assert_eq!(
            classify(None, None, Some("/Users/a/Documents/archive"), "Python").2,
            "Directory"
        );
        assert_eq!(classify(None, None, Some("/"), "launchd").2, "Process");
    }
}

#[cfg(test)]
mod accounting_tests {
    use super::*;
    fn process(id: &str, footprint: Option<u64>) -> Process {
        Process {
            id: id.into(),
            pid: 1,
            ppid: None,
            started: 1,
            name: "test".into(),
            executable: None,
            cwd: None,
            cpu: 50.,
            resident: 20,
            footprint,
            group_id: "project:/a".into(),
            group_name: "a".into(),
            kind: "Project".into(),
            evidence: "fixture".into(),
            service: None,
            ports: vec!["*:3000".into()],
            read_bytes: 0,
            written_bytes: 0,
        }
    }
    #[test]
    fn inaccessible_footprint_is_partial_not_zero_measurement() {
        let groups = aggregate(&[
            process("1", Some(100)),
            process("2", None),
            process("3", Some(0)),
        ]);
        assert_eq!(groups[0].count, 3);
        assert_eq!(groups[0].measured, 2);
        assert_eq!(groups[0].footprint, 100);
        assert_eq!(groups[0].resident, 60);
        assert_eq!(groups[0].cpu, 150.);
        assert_eq!(groups[0].ports.len(), 1);
    }
    #[test]
    fn unknown_only_group_stays_unknown_in_coverage() {
        let groups = aggregate(&[process("1", None)]);
        assert_eq!(groups[0].measured, 0);
    }
}
