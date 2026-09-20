mod discovery;
mod history;
mod model;
use model::*;
use std::{
    collections::HashMap,
    io::{self, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn usage(_pid: u32) -> Option<(u64, u64)> {
    #[cfg(target_os = "macos")]
    unsafe {
        let mut r: libc::rusage_info_v0 = std::mem::zeroed();
        let rc =
            libc::proc_pid_rusage(_pid as i32, 0, (&mut r as *mut libc::rusage_info_v0).cast());
        if rc == 0 {
            return Some((r.ri_phys_footprint, r.ri_proc_start_abstime));
        }
    }
    None
}
fn project(cwd: &Path) -> Option<String> {
    let mut dir = Some(cwd);
    for _ in 0..16 {
        let p = dir?;
        // A home dotfiles repository or package-manager checkout is not a project attribution.
        if std::env::var_os("HOME").as_deref() == Some(p.as_os_str())
            || p == Path::new("/opt/homebrew")
            || p == Path::new("/usr/local/Homebrew")
        {
            return None;
        }
        if p.join(".git").exists() {
            return Some(p.to_string_lossy().into_owned());
        }
        dir = p.parent();
    }
    None
}
fn refresh(sys: &mut System) {
    sys.refresh_cpu_usage();
    sys.refresh_memory();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::new()
            .with_cpu()
            .with_memory()
            .with_disk_usage()
            .with_exe(UpdateKind::OnlyIfNotSet)
            .with_cwd(UpdateKind::Always),
    );
}
fn database() -> PathBuf {
    std::env::var_os("DOT_RESOURCE_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
                .join("Library/Application Support/DOT Terminal/resources")
        })
        .join("history.sqlite")
}
#[tokio::main(flavor = "current_thread")]
async fn main() {
    // All locally retained paths and history are private to this account.
    unsafe {
        libc::umask(0o077);
    }
    if let Err(e) = run().await {
        eprintln!("Resource collector: {e}");
        std::process::exit(1);
    }
}
async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--help") {
        println!("resource-collector [--samples N] [--interval SECONDS] [--history-group ID]\nRead-only JSONL telemetry; 5-second default. No process controls.\nDOT_RESOURCE_DATA_DIR overrides the local SQLite directory.");
        return Ok(());
    }
    let value = |flag: &str| {
        args.iter()
            .position(|s| s == flag)
            .and_then(|i| args.get(i + 1))
    };
    let samples = value("--samples")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let interval = value("--interval")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(5)
        .clamp(2, 60);
    let path = database();
    let mut history_error = None;
    let mut db = match std::fs::create_dir_all(path.parent().unwrap())
        .map_err(|e| e.to_string())
        .and_then(|_| history::History::open(&path).map_err(|e| e.to_string()))
    {
        Ok(db) => Some(db),
        Err(e) => {
            history_error = Some(format!("History unavailable: {e}"));
            None
        }
    };
    if let Some(id) = value("--history-group") {
        let db = db.as_ref().ok_or("history unavailable")?;
        println!("{}", serde_json::to_string(&db.group_points(id)?)?);
        return Ok(());
    }
    let parent = unsafe { libc::getppid() };
    let mut sys = System::new();
    refresh(&mut sys);
    // sysinfo creates process records on the first pass; an update seeds CPU counters.
    // Seed before waiting so the first published sample is a real delta, not all zeros.
    refresh(&mut sys);
    tokio::time::sleep(Duration::from_secs(1)).await;
    let mut count = 0;
    let mut last_discovery = None;
    let mut attribution_at = None;
    let mut jobs = vec![];
    let mut ports = HashMap::new();
    let mut discovery_errors = vec![];
    let mut discovery_starts: HashMap<u32, u64> = HashMap::new();
    let mut project_cache: HashMap<PathBuf, Option<String>> = HashMap::new();
    let mut last_record = None;
    loop {
        if unsafe { libc::getppid() } != parent {
            break;
        }
        let tick = Instant::now();
        refresh(&mut sys);
        if last_discovery
            .map(|i: Instant| i.elapsed() >= Duration::from_secs(30))
            .unwrap_or(true)
        {
            let result = discovery::discover().await;
            jobs = result.0;
            ports = result.1;
            discovery_errors = result.2;
            // Bind cached attribution to observed process start times, not reusable PIDs.
            discovery_starts = sys
                .processes()
                .iter()
                .map(|(pid, p)| (pid.as_u32(), p.start_time()))
                .collect();
            attribution_at = Some(now());
            last_discovery = Some(Instant::now());
            project_cache.clear();
        }
        let mut procs = vec![];
        for (pid, p) in sys.processes() {
            let pid = pid.as_u32();
            let native = usage(pid);
            let exe = p.exe().map(|p| p.to_string_lossy().into_owned());
            let cwd = p.cwd().map(|p| p.to_string_lossy().into_owned());
            let root = p.cwd().and_then(|dir| {
                project_cache
                    .entry(dir.into())
                    .or_insert_with(|| project(dir))
                    .clone()
            });
            let name = p.name().to_string_lossy().into_owned();
            // Framework-packaged interpreters are not user applications.
            let app_exe = exe.as_deref().filter(|s| {
                s.starts_with("/Applications/")
                    || s.starts_with("/System/Applications/")
                    || s.contains("/Applications/")
            });
            let (mut group_id, mut group_name, mut kind, mut evidence) =
                model::classify(app_exe, root.as_deref(), cwd.as_deref(), &name);
            let fresh = discovery_starts.get(&pid) == Some(&p.start_time());
            let service = if fresh {
                jobs.iter()
                    .find(|j| j.pid == Some(pid))
                    .map(|j| j.label.clone())
            } else {
                None
            };
            if kind == "Process" {
                if let Some(label) = &service {
                    group_id = format!("service:{label}");
                    group_name = label.clone();
                    kind = "Service".into();
                    evidence = "PID matches a job in this login session's launchd domain".into();
                } else if let Some(exe) = &exe {
                    group_id = format!("executable:{exe}");
                }
            }
            procs.push(Process {
                id: format!(
                    "{}:{pid}:{}",
                    System::boot_time(),
                    native.map(|n| n.1).unwrap_or(p.start_time())
                ),
                pid,
                ppid: p.parent().map(|p| p.as_u32()),
                started: p.start_time(),
                name,
                executable: exe,
                cwd,
                cpu: p.cpu_usage() as f64,
                resident: p.memory(),
                virtual_memory: Some(p.virtual_memory()),
                footprint: native.map(|n| n.0),
                group_id,
                group_name,
                kind,
                evidence,
                service,
                ports: if fresh {
                    ports.get(&pid).cloned().unwrap_or_default()
                } else {
                    vec![]
                },
                read_bytes: p.disk_usage().total_read_bytes,
                written_bytes: p.disk_usage().total_written_bytes,
            });
        }
        procs.sort_by(|a, b| b.footprint.cmp(&a.footprint).then(a.pid.cmp(&b.pid)));
        let mut issues = discovery_errors.clone();
        if let Some(e) = &history_error {
            issues.push(e.clone());
        }
        let mut snapshot = Snapshot {
            schema: 1,
            at: now(),
            state: if issues.is_empty() {
                "alive"
            } else {
                "degraded"
            }
            .into(),
            issues,
            host: System::host_name().unwrap_or_else(|| "This Mac".into()),
            cores: sys.cpus().len(),
            cpu: sys.global_cpu_usage() as f64,
            memory_total: sys.total_memory(),
            memory_used: sys.used_memory(),
            swap_used: sys.used_swap(),
            uptime: System::uptime(),
            collector_pid: std::process::id(),
            duration_ms: 0,
            attribution_at,
            groups: aggregate(&procs),
            processes: procs,
            services: jobs.clone(),
            history: vec![],
        };
        if last_record
            .map(|i: Instant| i.elapsed() >= Duration::from_secs(60))
            .unwrap_or(true)
        {
            if let Some(db) = &mut db {
                match db.record(&snapshot) {
                    Ok(()) => {
                        history_error = None;
                        last_record = Some(Instant::now());
                    }
                    Err(e) => {
                        let msg = format!("History write failed: {e}");
                        history_error = Some(msg.clone());
                        snapshot.issues.push(msg);
                        snapshot.state = "degraded".into();
                    }
                }
            }
        }
        if let Some(db) = &db {
            match db.points() {
                Ok(h) => snapshot.history = h,
                Err(e) => {
                    snapshot.issues.push(format!("History read failed: {e}"));
                    snapshot.state = "degraded".into();
                }
            }
        }
        snapshot.duration_ms = tick.elapsed().as_millis() as u64;
        // A closed UI pipe ends the collector; it never becomes an orphan service.
        let stdout = io::stdout();
        let mut out = stdout.lock();
        if writeln!(out, "{}", serde_json::to_string(&snapshot)?)
            .and_then(|_| out.flush())
            .is_err()
        {
            break;
        }
        drop(out);
        count += 1;
        if samples > 0 && count >= samples {
            break;
        }
        tokio::time::sleep(Duration::from_secs(interval).saturating_sub(tick.elapsed())).await;
    }
    Ok(())
}
#[cfg(test)]
mod native_tests {
    #[test]
    fn own_footprint_is_available_and_missing_pid_is_unknown() {
        #[cfg(target_os = "macos")]
        {
            assert!(super::usage(std::process::id()).unwrap().0 > 0);
            assert!(super::usage(i32::MAX as u32).is_none());
        }
    }
}
