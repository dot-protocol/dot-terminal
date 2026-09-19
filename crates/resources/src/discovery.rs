use crate::model::Service;
use std::{collections::HashMap, time::Duration};
use tokio::process::Command;

async fn read(program: &str, args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new(program);
    cmd.args(args).kill_on_drop(true);
    let out = tokio::time::timeout(Duration::from_secs(4), cmd.output())
        .await
        .map_err(|_| format!("{program} timed out"))?
        .map_err(|e| format!("{program}: {e}"))?;
    // lsof returns 1 when there are no matching sockets.
    let empty_lsof =
        program.ends_with("lsof") && out.status.code() == Some(1) && out.stdout.is_empty();
    if !(out.status.success() || empty_lsof) {
        return Err(format!("{program} returned {}", out.status));
    }
    String::from_utf8(out.stdout).map_err(|_| format!("{program} returned invalid text"))
}
pub fn services(text: &str) -> Vec<Service> {
    text.lines()
        .filter_map(|line| {
            let mut f = line.split_whitespace();
            let pid = f.next()?;
            let exit: i32 = f.next()?.parse().ok()?;
            let label = f.next()?;
            Some(Service {
                label: label.into(),
                pid: pid.parse().ok(),
                last_exit: exit,
            })
        })
        .collect()
}
pub fn ports(text: &str) -> HashMap<u32, Vec<String>> {
    let mut result: HashMap<u32, Vec<String>> = HashMap::new();
    let mut pid = None;
    for line in text.lines() {
        if let Some(s) = line.strip_prefix('p') {
            pid = s.parse().ok();
        }
        if let (Some(p), Some(socket)) = (pid, line.strip_prefix('n')) {
            result.entry(p).or_default().push(socket.into());
        }
    }
    for v in result.values_mut() {
        v.sort();
        v.dedup();
    }
    result
}
pub async fn discover() -> (Vec<Service>, HashMap<u32, Vec<String>>, Vec<String>) {
    let mut errors = vec![];
    let jobs = match read("/bin/launchctl", &["list"]).await {
        Ok(s) => services(&s),
        Err(e) => {
            errors.push(e);
            vec![]
        }
    };
    let sockets = match read("/usr/sbin/lsof", &["-nP", "-iTCP", "-sTCP:LISTEN", "-Fpn"]).await {
        Ok(s) => ports(&s),
        Err(e) => {
            errors.push(e);
            HashMap::new()
        }
    };
    (jobs, sockets, errors)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exited_jobs_are_not_running_and_headers_are_ignored() {
        let s = services("PID Status Label\n- 78 service.failed\n123 0 service.running\n");
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].pid, None);
        assert_eq!(s[0].last_exit, 78);
        assert_eq!(s[1].pid, Some(123));
    }
    #[test]
    fn sockets_are_attributed_to_pid_and_deduplicated() {
        let p = ports("p12\nn127.0.0.1:3000\nn127.0.0.1:3000\np13\nn[::1]:7431\n");
        assert_eq!(p[&12], vec!["127.0.0.1:3000"]);
        assert_eq!(p[&13], vec!["[::1]:7431"]);
    }
}
