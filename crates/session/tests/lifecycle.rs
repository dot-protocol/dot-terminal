#![cfg(unix)]
use dot_terminal_protocol::{Operation, Request, Response, VERSION, read_message, write_message};
use std::{
    io::Write,
    os::unix::{fs::PermissionsExt, net::UnixStream},
    process::Command,
    time::{Duration, Instant},
};
struct Session {
    dir: tempfile::TempDir,
    id: String,
}
impl Session {
    fn new(command: &[&str]) -> Self {
        let dir = tempfile::Builder::new()
            .prefix("dt-")
            .tempdir_in("/tmp")
            .unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let out = Command::new(env!("CARGO_BIN_EXE_dot-terminal"))
            .arg("--state-dir")
            .arg(dir.path())
            .arg("new")
            .arg("--")
            .args(command)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        Self {
            dir,
            id: String::from_utf8(out.stdout).unwrap().trim().into(),
        }
    }
    fn call_version(&self, version: u16, operation: Operation) -> Response {
        let mut s = UnixStream::connect(self.dir.path().join(format!("{}.sock", self.id))).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        write_message(&mut s, &Request { version, operation }).unwrap();
        read_message(&mut s).unwrap()
    }
    fn call(&self, op: Operation) -> Response {
        self.call_version(VERSION, op)
    }
    fn acquire(&self, takeover: bool) -> u64 {
        match self.call(Operation::Acquire { takeover }) {
            Response::Lease { generation } => generation,
            x => panic!("{x:?}"),
        }
    }
    fn wait_output(&self, needle: &[u8]) -> Vec<u8> {
        let deadline = Instant::now() + Duration::from_secs(4);
        while Instant::now() < deadline {
            match self.call(Operation::Read { after: 0 }) {
                Response::Output { data, .. }
                    if data.windows(needle.len()).any(|w| w == needle) =>
                {
                    return data;
                }
                _ => {}
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("output not observed")
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.call(Operation::Stop {});
        std::thread::sleep(Duration::from_millis(50));
    }
}
#[test]
fn outlives_creator_and_preserves_command_arguments() {
    let s = Session::new(&[
        "/bin/sh",
        "-c",
        "printf '%s' \"$1\"; exec cat",
        "sh",
        "one argument with spaces",
    ]);
    s.wait_output(b"one argument with spaces");
    assert!(matches!(
        s.call(Operation::Status {}),
        Response::Status { exited: false, .. }
    ));
}
#[test]
fn handoff_rejects_old_input_and_resize() {
    let s = Session::new(&["/bin/cat"]);
    let a = s.acquire(false);
    let b = s.acquire(true);
    assert!(matches!(
        s.call(Operation::Input {
            generation: a,
            sequence: 1,
            data: b"OLD\n".to_vec()
        }),
        Response::Error { .. }
    ));
    assert!(matches!(
        s.call(Operation::Resize {
            generation: a,
            cols: 90,
            rows: 30
        }),
        Response::Error { .. }
    ));
    assert!(matches!(
        s.call(Operation::Input {
            generation: b,
            sequence: 1,
            data: b"NEW\n".to_vec()
        }),
        Response::Ack { duplicate: false }
    ));
    let bytes = s.wait_output(b"NEW");
    assert!(!bytes.windows(3).any(|w| w == b"OLD"));
}
#[test]
fn reconnect_deduplicates_and_refuses_conflicting_request() {
    let s = Session::new(&["/bin/cat"]);
    let g = s.acquire(false);
    for expected in [false, true] {
        assert!(
            matches!(s.call(Operation::Input{generation:g,sequence:1,data:b"hello\n".to_vec()}),Response::Ack{duplicate} if duplicate==expected)
        );
    }
    assert!(matches!(
        s.call(Operation::Input {
            generation: g,
            sequence: 1,
            data: b"different\n".to_vec()
        }),
        Response::Error { .. }
    ));
}
#[test]
fn malformed_client_does_not_kill_keeper() {
    let s = Session::new(&["/bin/cat"]);
    let mut c = UnixStream::connect(s.dir.path().join(format!("{}.sock", s.id))).unwrap();
    c.write_all(&u32::MAX.to_be_bytes()).unwrap();
    assert!(matches!(
        s.call_version(99, Operation::Status {}),
        Response::Error { .. }
    ));
    assert!(matches!(
        s.call(Operation::Status {}),
        Response::Status { .. }
    ));
}
#[test]
fn exit_retains_output_until_explicit_stop() {
    let s = Session::new(&["/bin/sh", "-c", "printf finished"]);
    s.wait_output(b"finished");
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if matches!(
            s.call(Operation::Status {}),
            Response::Status { exited: true, .. }
        ) {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    s.wait_output(b"finished");
}
#[test]
fn refuses_insecure_state_directory() {
    let d = tempfile::Builder::new()
        .prefix("dt-")
        .tempdir_in("/tmp")
        .unwrap();
    std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_dot-terminal"))
        .arg("--state-dir")
        .arg(d.path())
        .arg("list")
        .output()
        .unwrap();
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("0700"));
}

#[test]
fn screen_reconnect_restores_parsed_state_and_dimensions() {
    let s = Session::new(&[
        "/bin/sh",
        "-c",
        "printf 'old text\\r\\033[2Kcurrent'; exec cat",
    ]);
    s.wait_output(b"current");
    let g = s.acquire(false);
    assert!(matches!(
        s.call(Operation::Resize {
            generation: g,
            cols: 48,
            rows: 24
        }),
        Response::Ack { .. }
    ));
    match s.call(Operation::Screen {}) {
        Response::Screen {
            cols, rows, lines, ..
        } => {
            assert_eq!((cols, rows), (48, 24));
            assert_eq!(lines[0], "current");
        }
        other => panic!("{other:?}"),
    }
    s.call(Operation::Release { generation: g });
    let _ = s.acquire(false);
    match s.call(Operation::Screen {}) {
        Response::Screen { lines, .. } => assert_eq!(lines[0], "current"),
        other => panic!("{other:?}"),
    }
}
