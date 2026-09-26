#![cfg(unix)]
//! Conformance rig: the stream properties every DOT client relies on, checked against a real keeper
//! and a real PTY. Each test names the promise it holds the keeper to. A keeper that fails one of
//! these is not shipped, whatever its UI does on top.
use dot_terminal_protocol::{Operation, Request, Response, VERSION, read_message, write_message};
use std::{
    os::unix::{fs::PermissionsExt, net::UnixStream},
    process::Command,
    sync::Arc,
    time::{Duration, Instant},
};

struct Session {
    dir: tempfile::TempDir,
    id: String,
}
impl Session {
    fn new(script: &str) -> Self {
        let dir = tempfile::Builder::new()
            .prefix("dt-rig-")
            .tempdir_in("/tmp")
            .unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let out = Command::new(env!("CARGO_BIN_EXE_dot-terminal"))
            .arg("--state-dir")
            .arg(dir.path())
            .args(["new", "--", "/bin/sh", "-c", script])
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
    fn call(&self, operation: Operation) -> Response {
        let mut s = UnixStream::connect(self.dir.path().join(format!("{}.sock", self.id))).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        write_message(
            &mut s,
            &Request {
                version: VERSION,
                operation,
            },
        )
        .unwrap();
        read_message(&mut s).unwrap()
    }
    fn frame(&self, after: u64) -> Frame {
        match self.call(Operation::ReadFrame { after }) {
            Response::Frame {
                start,
                next,
                gap,
                data,
                cols,
                rows,
                geometry_epoch,
                incarnation,
                ..
            } => Frame {
                start,
                next,
                gap,
                data,
                cols,
                rows,
                epoch: geometry_epoch,
                incarnation,
            },
            x => panic!("expected a frame, got {x:?}"),
        }
    }
    fn acquire(&self) -> u64 {
        match self.call(Operation::Acquire { takeover: true }) {
            Response::Lease { generation } => generation,
            x => panic!("{x:?}"),
        }
    }
    /// Read from `after` until `done` holds for everything read so far, checking the stream promise
    /// on every frame: each frame starts where the previous one ended, or says `gap`.
    fn read_until(&self, mut after: u64, done: impl Fn(&[u8]) -> bool) -> (Vec<u8>, Vec<Frame>) {
        let deadline = Instant::now() + Duration::from_secs(20);
        let (mut bytes, mut frames) = (Vec::new(), Vec::new());
        while !done(&bytes) {
            assert!(
                Instant::now() < deadline,
                "stream stalled after {} bytes",
                bytes.len()
            );
            let f = self.frame(after);
            assert!(
                f.next >= f.start && f.next - f.start == f.data.len() as u64,
                "frame bounds lie: {f:?}"
            );
            if f.data.is_empty() {
                std::thread::sleep(Duration::from_millis(5));
                continue;
            }
            assert!(
                f.start == after || f.gap,
                "bytes {after}..{} were skipped without a gap",
                f.start
            );
            bytes.extend_from_slice(&f.data);
            after = f.next;
            frames.push(f);
        }
        (bytes, frames)
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        // Tolerates a keeper that already left (the reaper tests remove sockets on purpose).
        if let Ok(mut s) = UnixStream::connect(self.dir.path().join(format!("{}.sock", self.id))) {
            let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
            let _ = write_message(
                &mut s,
                &Request {
                    version: VERSION,
                    operation: Operation::Stop {},
                },
            );
            let _: Result<Response, _> = read_message(&mut s);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
#[derive(Debug)]
struct Frame {
    start: u64,
    next: u64,
    gap: bool,
    data: Vec<u8>,
    cols: u16,
    rows: u16,
    epoch: u64,
    incarnation: String,
}
fn ends_with(needle: &'static [u8]) -> impl Fn(&[u8]) -> bool {
    move |b: &[u8]| b.windows(needle.len()).any(|w| w == needle)
}

/// Promise: every attached view sees the same bytes in the same order. Three readers pull the same
/// 20,000-line burst concurrently; each gets the whole stream, and the three are byte-identical.
#[test]
fn three_readers_see_one_identical_stream() {
    let s = Arc::new(Session::new(
        "i=0; while [ $i -lt 20000 ]; do echo \"line $i\"; i=$((i+1)); done; echo DONE; exec cat",
    ));
    let readers: Vec<_> = (0..3)
        .map(|_| {
            let s = s.clone();
            std::thread::spawn(move || s.read_until(0, ends_with(b"DONE\r\n")).0)
        })
        .collect();
    let streams: Vec<Vec<u8>> = readers.into_iter().map(|r| r.join().unwrap()).collect();
    for (n, other) in streams.iter().enumerate().skip(1) {
        assert!(
            other == &streams[0],
            "reader {n} and reader 0 received different bytes from the same keeper"
        );
    }
    let expected: String = (0..20000)
        .map(|i| format!("line {i}\r\n"))
        .collect::<String>()
        + "DONE\r\n";
    for (n, got) in streams.iter().enumerate() {
        // macOS's tty driver re-emits the CR of an ONLCR "\r\n" when its output queue fills while a
        // reader is slow: a bare PTY with a slow reader shows "\r\r\n" in 20/20 runs on macOS and 0/20
        // on Linux (2026-09-26). The keeper stores what the PTY produced, so only there, and only
        // that artifact, is folded before comparing content. The readers must still agree exactly.
        let folded;
        let got: &[u8] = if cfg!(target_os = "macos") {
            folded = String::from_utf8_lossy(got)
                .replace("\r\r\n", "\r\n")
                .into_bytes();
            &folded
        } else {
            got
        };
        let got = &got[..expected.len().min(got.len())];
        if got != expected.as_bytes() {
            let at = got
                .iter()
                .zip(expected.as_bytes())
                .position(|(a, b)| a != b)
                .unwrap_or(got.len());
            let lo = at.saturating_sub(40);
            panic!(
                "reader {n} diverged at byte {at}: got {:?} expected {:?}",
                String::from_utf8_lossy(&got[lo..(at + 40).min(got.len())]),
                &expected[lo..(at + 40).min(expected.len())]
            );
        }
    }
}

/// Promise: the keeper is byte-preserving. Multibyte UTF-8, emoji and escape sequences sent one byte
/// per input message (every split point) come back out of a raw `cat` exactly as sent.
#[test]
fn input_split_at_every_byte_boundary_round_trips_exactly() {
    let s = Session::new("stty raw -echo; printf READY; exec cat");
    let (head, _) = s.read_until(0, ends_with(b"READY"));
    let after = head.len() as u64;
    let g = s.acquire();
    let sample = "héllo 世界 🦀 \x1b[31mred\x1b[0m ✓ \x1b]0;title\x07 end"
        .as_bytes()
        .to_vec();
    for (i, byte) in sample.iter().enumerate() {
        match s.call(Operation::Input {
            generation: g,
            sequence: i as u64 + 1,
            data: vec![*byte],
        }) {
            Response::Ack { duplicate: false } => {}
            x => panic!("byte {i} not accepted: {x:?}"),
        }
    }
    let (out, _) = s.read_until(after, move |b: &[u8]| b.len() >= sample.len());
    assert_eq!(
        out,
        "héllo 世界 🦀 \x1b[31mred\x1b[0m ✓ \x1b]0;title\x07 end".as_bytes()
    );
}

/// Promise: a view that attaches while output is flowing never gets fabricated continuity. Either its
/// first frame starts at 0, or it is flagged `gap`; after that every frame is contiguous, and the
/// counter lines it sees are consecutive.
#[test]
fn late_attach_during_output_is_contiguous_or_says_gap() {
    let s = Session::new(
        "i=0; while [ $i -lt 400000 ]; do echo \"n $i\"; i=$((i+1)); done; echo DONE; exec cat",
    );
    std::thread::sleep(Duration::from_millis(300)); // attach late, mid-burst
    let first = s.frame(0);
    assert!(
        first.start == 0 || first.gap,
        "attach at {} without a gap flag",
        first.start
    );
    let (bytes, frames) = s.read_until(first.next, ends_with(b"DONE\r\n"));
    for w in frames.windows(2) {
        assert!(
            w[1].start == w[0].next || w[1].gap,
            "hole between {} and {}",
            w[0].next,
            w[1].start
        );
        assert_eq!(w[1].incarnation, w[0].incarnation);
    }
    // Lines are consecutive wherever no gap was reported.
    if !frames.iter().any(|f| f.gap) {
        let text = String::from_utf8_lossy(&bytes);
        let nums: Vec<u64> = text
            .split("\r\n")
            .filter_map(|l| l.strip_prefix("n ")?.parse().ok())
            .collect();
        for w in nums.windows(2) {
            assert_eq!(
                w[1],
                w[0] + 1,
                "line {} followed by {} with no gap reported",
                w[0],
                w[1]
            );
        }
    }
}

/// Promise: resize history is ordered. Fifty resizes while output flows: the epoch never goes back,
/// bytes are never labelled with a grid from a later epoch, and the stream ends on the last size.
#[test]
fn resize_history_is_ordered_under_output() {
    let s = Arc::new(Session::new("while :; do printf 'x'; sleep 0.002; done"));
    let g = s.acquire();
    let sizes: Vec<(u16, u16)> = (0..50)
        .map(|i| (80 + (i % 7) * 10, 24 + (i % 5) * 4))
        .collect();
    let reader = {
        let s = s.clone();
        std::thread::spawn(move || {
            let mut after = 0;
            let mut seen = Vec::new();
            let deadline = Instant::now() + Duration::from_secs(10);
            while Instant::now() < deadline {
                let f = s.frame(after);
                if !f.data.is_empty() {
                    after = f.next;
                    seen.push((f.epoch, f.cols, f.rows));
                    if f.epoch >= 50 {
                        break;
                    }
                }
                std::thread::sleep(Duration::from_millis(3));
            }
            seen
        })
    };
    for (cols, rows) in &sizes {
        match s.call(Operation::Resize {
            generation: g,
            cols: *cols,
            rows: *rows,
        }) {
            Response::Ack { .. } => {}
            x => panic!("resize refused: {x:?}"),
        }
        std::thread::sleep(Duration::from_millis(8));
    }
    let seen = reader.join().unwrap();
    assert!(!seen.is_empty(), "no output observed");
    for w in seen.windows(2) {
        assert!(
            w[1].0 >= w[0].0,
            "geometry epoch went backwards: {:?} then {:?}",
            w[0],
            w[1]
        );
    }
    let last = seen.last().unwrap();
    assert_eq!(last.0, 50, "stream never reached the last resize: {last:?}");
    assert_eq!(
        (last.1, last.2),
        *sizes.last().unwrap(),
        "latest frame is not on the last grid"
    );
}

/// Promise: a keystroke is never refused because of what other views are doing. While the controller
/// types 400 single-byte inputs, views check control, resize and pull frames in tight loops; every
/// keystroke is accepted and the output is exactly what was typed.
#[test]
fn input_is_never_refused_while_other_views_check_control() {
    let s = Arc::new(Session::new("stty raw -echo; printf READY; exec cat"));
    let (head, _) = s.read_until(0, ends_with(b"READY"));
    let g = s.acquire();
    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    // What real views do while one of them types: check control, resize the window, pull frames.
    let checkers: Vec<_> = (0..3)
        .map(|role| {
            let (s, done) = (s.clone(), done.clone());
            std::thread::spawn(move || {
                let mut n = 0u16;
                while !done.load(std::sync::atomic::Ordering::Relaxed) {
                    n = n.wrapping_add(1);
                    let _ = match role {
                        0 => s.call(Operation::CheckControl { generation: g }),
                        1 => s.call(Operation::Resize {
                            generation: g,
                            cols: 80 + n % 40,
                            rows: 24 + n % 10,
                        }),
                        _ => s.call(Operation::ReadFrame { after: 0 }),
                    };
                }
            })
        })
        .collect();
    let typed: Vec<u8> = (0..400).map(|i| b'a' + (i % 26) as u8).collect();
    let mut refused = Vec::new();
    for (i, b) in typed.iter().enumerate() {
        match s.call(Operation::Input {
            generation: g,
            sequence: i as u64 + 1,
            data: vec![*b],
        }) {
            Response::Ack { duplicate: false } => {}
            x => refused.push((i, format!("{x:?}"))),
        }
    }
    done.store(true, std::sync::atomic::Ordering::Relaxed);
    for c in checkers {
        c.join().unwrap();
    }
    assert!(
        refused.is_empty(),
        "{} of 400 keystrokes refused, first: {:?}",
        refused.len(),
        refused.first()
    );
    let (out, _) = s.read_until(head.len() as u64, |b: &[u8]| b.len() >= 400);
    assert_eq!(out, typed);
}

/// Promise: a retried input is recognised, not replayed and not refused as out of order. Any of the
/// last 64 inputs retried with the same bytes is a duplicate; the same sequence with other bytes is
/// a conflict.
#[test]
fn retries_within_the_window_are_duplicates_and_conflicts_are_refused() {
    let s = Session::new("stty raw -echo; printf READY; exec cat");
    let (head, _) = s.read_until(0, ends_with(b"READY"));
    let g = s.acquire();
    let input = |seq: u64, byte: u8| {
        s.call(Operation::Input {
            generation: g,
            sequence: seq,
            data: vec![byte],
        })
    };
    for seq in 1..=10 {
        assert!(matches!(
            input(seq, b'0' + seq as u8 % 10),
            Response::Ack { duplicate: false }
        ));
    }
    assert!(
        matches!(input(5, b'5'), Response::Ack { duplicate: true }),
        "retry of seq 5 was not a duplicate"
    );
    assert!(
        matches!(input(1, b'1'), Response::Ack { duplicate: true }),
        "retry of seq 1 was not a duplicate"
    );
    assert!(
        matches!(input(5, b'x'), Response::Error { .. }),
        "seq 5 with other bytes was accepted"
    );
    assert!(matches!(
        input(11, b'!'),
        Response::Ack { duplicate: false }
    ));
    let (out, _) = s.read_until(head.len() as u64, |b: &[u8]| b.len() >= 11);
    assert_eq!(
        out, b"1234567890!",
        "a retry was written twice or a byte was lost"
    );
}

/// Promise: a shell never inherits the identity of the session that launched its keeper. A gateway
/// started from inside Claude Code (or tmux, iTerm, WezTerm…) must not hand that session's bridge id,
/// messaging socket or messaging token to every terminal it creates; the user's own variables stay.
#[test]
fn a_shell_does_not_inherit_the_launching_session() {
    let dir = tempfile::Builder::new()
        .prefix("dt-rig-")
        .tempdir_in("/tmp")
        .unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_dot-terminal"))
        .arg("--state-dir")
        .arg(dir.path())
        .args([
            "new",
            "--",
            "/bin/sh",
            "-c",
            "env | cut -d= -f1 | sort | tr '\\n' ' '; echo; echo ENV-DONE; exec cat",
        ])
        .env("CLAUDE_CODE_MESSAGING_TOKEN", "must-not-leak")
        .env("CLAUDE_CODE_BRIDGE_SESSION_ID", "parent")
        .env("CLAUDE_CODE_SESSION_ID", "parent")
        .env("CLAUDE_PID", "1")
        .env("TERM_SESSION_ID", "w0t0p0")
        .env("TMUX", "/tmp/tmux-1/default,1,0")
        .env("CLAUDECODE", "1")
        .env("ITERM_SESSION_ID", "w0t0p0:x")
        .env("LC_TERMINAL", "iTerm2")
        .env("TERM_PROGRAM", "iTerm.app")
        .env("DOT_RIG_USER_VAR", "kept")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = Session {
        dir,
        id: String::from_utf8(out.stdout).unwrap().trim().into(),
    };
    let (bytes, _) = s.read_until(0, ends_with(b"ENV-DONE"));
    let names = String::from_utf8_lossy(&bytes);
    for leaked in [
        "CLAUDE_CODE_MESSAGING_TOKEN",
        "CLAUDE_CODE_BRIDGE_SESSION_ID",
        "CLAUDE_CODE_SESSION_ID",
        "CLAUDE_PID",
        "CLAUDECODE",
        "TERM_SESSION_ID",
        "TMUX",
        "ITERM_SESSION_ID",
        "LC_TERMINAL",
    ] {
        assert!(
            !names.split_whitespace().any(|n| n == leaked),
            "{leaked} leaked into the shell: {names}"
        );
    }
    assert!(
        names.split_whitespace().any(|n| n == "DOT_RIG_USER_VAR"),
        "the user's own variable was dropped: {names}"
    );
    assert!(!names.contains("must-not-leak"));
    let s2 = Session::new("printf 'program=%s\\n' \"$TERM_PROGRAM\"; echo PROG-DONE; exec cat");
    let (prog, _) = s2.read_until(0, ends_with(b"PROG-DONE"));
    assert!(
        String::from_utf8_lossy(&prog).contains("program=DOT-Terminal"),
        "the shell was not told it runs in DOT"
    );
}

fn spawn_env(script: &str, env: &[(&str, &str)]) -> Session {
    let dir = tempfile::Builder::new()
        .prefix("dt-rig-")
        .tempdir_in("/tmp")
        .unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut c = Command::new(env!("CARGO_BIN_EXE_dot-terminal"));
    c.arg("--state-dir")
        .arg(dir.path())
        .args(["new", "--", "/bin/sh", "-c", script]);
    for (k, v) in env {
        c.env(k, v);
    }
    let out = c.output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    Session {
        dir,
        id: String::from_utf8(out.stdout).unwrap().trim().into(),
    }
}
/// Whether this session's keeper process is alive, judged without connecting to it (a connection
/// counts as a client and would reset its retention clock).
fn keeper_alive(id: &str) -> bool {
    Command::new("pgrep")
        .args(["-f", &format!("keeper {id}")])
        .output()
        .is_ok_and(|o| o.status.success())
}
fn wait_until(limit: Duration, f: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    f()
}
fn wait_exited(s: &Session) {
    assert!(wait_until(Duration::from_secs(5), || matches!(
        s.call(Operation::Status {}),
        Response::Status { exited: true, .. }
    )));
}

/// Promise: a session whose program has ended does not keep its keeper forever. It keeps its output
/// for the retention window, measured from the last client, then leaves.
#[test]
fn an_exited_session_leaves_after_its_retention_without_clients() {
    let s = spawn_env(
        "printf finished",
        &[("DOT_TERMINAL_EXITED_RETENTION_SECS", "2")],
    );
    s.read_until(0, ends_with(b"finished"));
    wait_exited(&s);
    assert!(keeper_alive(&s.id), "left before its retention");
    assert!(
        wait_until(Duration::from_secs(8), || !keeper_alive(&s.id)),
        "an exited session's keeper outlived its retention"
    );
    assert!(
        !s.dir.path().join(format!("{}.sock", s.id)).exists(),
        "left its socket behind"
    );
}

/// Promise: detached work is never reaped. A keeper whose program is still running stays, however
/// short the retention and however long nobody looks.
#[test]
fn a_running_session_is_never_reaped() {
    let s = spawn_env("exec cat", &[("DOT_TERMINAL_EXITED_RETENTION_SECS", "1")]);
    std::thread::sleep(Duration::from_secs(4));
    assert!(
        keeper_alive(&s.id),
        "a keeper with a live program was reaped"
    );
    assert!(matches!(
        s.call(Operation::Status {}),
        Response::Status { exited: false, .. }
    ));
}

/// Promise: an exited session that nobody can reach any more (its socket or state dir is gone) leaves
/// at once instead of waiting out the retention.
#[test]
fn an_exited_session_whose_socket_is_gone_leaves_at_once() {
    let s = spawn_env(
        "printf finished",
        &[("DOT_TERMINAL_EXITED_RETENTION_SECS", "3600")],
    );
    s.read_until(0, ends_with(b"finished"));
    wait_exited(&s);
    std::fs::remove_file(s.dir.path().join(format!("{}.sock", s.id))).unwrap();
    assert!(
        wait_until(Duration::from_secs(5), || !keeper_alive(&s.id)),
        "an unreachable exited keeper stayed"
    );
}

/// A subscription: one connection that stays open and is written frames as output lands.
struct Subscription(UnixStream);
impl Subscription {
    fn open(s: &Session, after: u64) -> Self {
        let mut c = UnixStream::connect(s.dir.path().join(format!("{}.sock", s.id))).unwrap();
        c.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        write_message(
            &mut c,
            &Request {
                version: VERSION,
                operation: Operation::Subscribe { after },
            },
        )
        .unwrap();
        Subscription(c)
    }
    /// The next pushed frame, or None when the keeper closed the subscription.
    fn next(&mut self) -> Option<Frame> {
        match read_message::<Response>(&mut self.0) {
            Ok(Response::Frame {
                start,
                next,
                gap,
                data,
                cols,
                rows,
                geometry_epoch,
                incarnation,
                exited,
            }) => {
                let _ = exited;
                Some(Frame {
                    start,
                    next,
                    gap,
                    data,
                    cols,
                    rows,
                    epoch: geometry_epoch,
                    incarnation,
                })
            }
            Ok(x) => panic!("expected a frame, got {x:?}"),
            Err(_) => None,
        }
    }
    fn until(&mut self, done: impl Fn(&[u8]) -> bool) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut after = None;
        while !done(&bytes) {
            let f = self.next().expect("subscription closed early");
            if let Some(a) = after {
                assert!(
                    f.start == a || f.gap,
                    "pushed bytes {a}..{} were skipped without a gap",
                    f.start
                );
            }
            after = Some(f.next);
            bytes.extend_from_slice(&f.data);
        }
        bytes
    }
}

/// Promise: a subscriber is told about output when it lands, not when it next asks. Measured as the
/// time from an accepted keystroke to the pushed frame carrying its echo (a lab number, not a claim
/// about any network).
#[test]
fn a_subscriber_gets_output_as_it_lands() {
    let s = Session::new("stty raw -echo; printf READY; exec cat");
    let mut sub = Subscription::open(&s, 0);
    sub.until(ends_with(b"READY"));
    let g = s.acquire();
    let mut waits = Vec::new();
    for i in 0..20u64 {
        std::thread::sleep(Duration::from_millis(30)); // idle between keystrokes, the case polling handles worst
        let t = Instant::now();
        assert!(matches!(
            s.call(Operation::Input {
                generation: g,
                sequence: i + 1,
                data: vec![b'a' + i as u8]
            }),
            Response::Ack { .. }
        ));
        sub.until(|b: &[u8]| !b.is_empty());
        waits.push(t.elapsed());
    }
    waits.sort();
    let p95 = waits[18];
    eprintln!(
        "keystroke to pushed echo: p50 {:?}, p95 {:?}",
        waits[10], p95
    );
    assert!(p95 < Duration::from_millis(50), "p95 {p95:?}");
}

/// Promise: every subscriber receives the same bytes in the same order, like every reader.
#[test]
fn three_subscribers_receive_one_identical_stream() {
    let s = Arc::new(Session::new(
        "sleep 0.3; i=0; while [ $i -lt 20000 ]; do echo \"line $i\"; i=$((i+1)); done; echo DONE; exec cat",
    ));
    let subs: Vec<_> = (0..3)
        .map(|_| {
            let s = s.clone();
            std::thread::spawn(move || Subscription::open(&s, 0).until(ends_with(b"DONE\r\n")))
        })
        .collect();
    let streams: Vec<Vec<u8>> = subs.into_iter().map(|t| t.join().unwrap()).collect();
    for (n, other) in streams.iter().enumerate().skip(1) {
        assert!(
            other == &streams[0],
            "subscriber {n} and subscriber 0 received different bytes"
        );
    }
    let text = String::from_utf8_lossy(&streams[0]).replace("\r\r\n", "\r\n");
    assert!(text.contains("line 0\r\nline 1\r\n") && text.contains("line 19999\r\nDONE"));
}

/// Promise: an idle subscription is not silent. A heartbeat (an empty frame) arrives so both ends can
/// tell a quiet session from a dead link.
#[test]
fn an_idle_subscription_gets_heartbeats() {
    let s = spawn_env(
        "printf READY; exec cat",
        &[("DOT_TERMINAL_HEARTBEAT_MS", "300")],
    );
    let mut sub = Subscription::open(&s, 0);
    sub.until(ends_with(b"READY"));
    let t = Instant::now();
    let beat = sub.next().expect("closed");
    assert!(beat.data.is_empty() && beat.start == beat.next, "{beat:?}");
    assert!(
        t.elapsed() < Duration::from_secs(2),
        "heartbeat took {:?}",
        t.elapsed()
    );
}

/// Promise: a subscription to a session whose program ended gets the remaining output, a final frame,
/// and is closed; it does not linger on a finished session.
#[test]
fn a_subscription_ends_when_the_session_does() {
    let s = Session::new("printf finished");
    wait_exited(&s);
    let mut sub = Subscription::open(&s, 0);
    let got = sub.until(ends_with(b"finished"));
    assert!(String::from_utf8_lossy(&got).contains("finished"));
    let t = Instant::now();
    while sub.next().is_some() {
        assert!(
            t.elapsed() < Duration::from_secs(2),
            "subscription stayed open on an ended session"
        );
    }
}
