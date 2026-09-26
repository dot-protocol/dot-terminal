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
        let _ = self.call(Operation::Stop {});
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
