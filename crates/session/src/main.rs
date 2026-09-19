#![cfg(unix)]
use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use dot_terminal_core::{Controller, History, InputDecision, Presence};
use dot_terminal_protocol::{Operation, Request, Response, VERSION};
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::{
    io::{Read, Write},
    os::unix::{
        fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
        net::{UnixListener, UnixStream},
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Parser)]
#[command(
    version,
    about = "Persistent local terminal sessions. Early runtime foundation."
)]
struct Cli {
    #[arg(long, global = true)]
    state_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Commands,
}
#[derive(Subcommand)]
enum Commands {
    /// Start a detached PTY session; prints its ID.
    New {
        #[arg(required = true, last = true)]
        command: Vec<String>,
    },
    /// List reachable sessions (including exited sessions until stopped).
    List,
    /// Inspect a live keeper.
    Status { id: String },
    /// Fetch bounded output as JSON, without executing terminal escape sequences.
    Read {
        id: String,
        #[arg(long, default_value_t = 0)]
        after: u64,
    },
    /// Interact with a session. Ctrl-] detaches. Current client replays bytes, not a full screen snapshot.
    Attach {
        id: String,
        #[arg(long)]
        takeover: bool,
        #[arg(long, conflicts_with = "takeover")]
        handoff_file: Option<PathBuf>,
    },
    /// Offer a two-minute handoff QR for a device already paired to this host.
    Handoff {
        id: String,
        #[arg(long)]
        node_id: String,
        #[arg(long)]
        qr: PathBuf,
        #[arg(long)]
        takeover: bool,
    },
    /// Terminate a session keeper and its direct child.
    Stop { id: String },
    #[command(hide = true)]
    Keeper {
        id: String,
        #[arg(required = true, last = true)]
        command: Vec<String>,
    },
}
fn main() -> Result<()> {
    let cli = Cli::parse();
    // Per-user runtime state is intentionally temporary until encrypted durable storage lands.
    let dir = cli.state_dir.unwrap_or_else(|| {
        PathBuf::from(format!("/tmp/dot-terminal-{}", unsafe { libc::geteuid() }))
    });
    secure_dir(&dir)?;
    match cli.command {
        Commands::New { command } => println!("{}", spawn(&dir, &command)?),
        Commands::Keeper { id, command } => keeper(&dir, &id, &command)?,
        Commands::List => {
            let mut paths = std::fs::read_dir(&dir)?
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "sock"))
                .collect::<Vec<_>>();
            paths.sort();
            for path in paths {
                let id = path.file_stem().unwrap().to_string_lossy();
                match call(&dir, &id, Operation::Status {}) {
                    Ok(r) => println!("{}", serde_json::to_string(&r)?),
                    Err(e) => eprintln!("unreachable keeper {id}: {e}"),
                }
            }
        }
        Commands::Status { id } => print_response(call(&dir, &id, Operation::Status {})?)?,
        Commands::Read { id, after } => {
            print_response(call(&dir, &id, Operation::Read { after })?)?
        }
        Commands::Stop { id } => print_response(call(&dir, &id, Operation::Stop {})?)?,
        Commands::Attach {
            id,
            takeover,
            handoff_file,
        } => attach(&dir, &id, takeover, handoff_file)?,
        Commands::Handoff {
            id,
            node_id,
            qr,
            takeover,
        } => offer_handoff(&dir, &id, &node_id, &qr, takeover)?,
    }
    Ok(())
}
fn print_response(r: Response) -> Result<()> {
    println!("{}", serde_json::to_string(&r)?);
    if let Response::Error { message } = r {
        bail!(message)
    }
    Ok(())
}
fn secure_dir(dir: &Path) -> Result<()> {
    if !dir.exists() {
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(dir)
            .context("create state directory (parent must exist)")?;
    }
    let m = std::fs::symlink_metadata(dir)?;
    if !m.is_dir()
        || m.file_type().is_symlink()
        || m.uid() != unsafe { libc::geteuid() }
        || m.mode() & 0o077 != 0
    {
        bail!("state directory must be a real directory owned by you with mode 0700");
    }
    Ok(())
}
fn socket(dir: &Path, id: &str) -> Result<PathBuf> {
    if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!("invalid session ID");
    }
    let p = dir.join(format!("{id}.sock"));
    if p.as_os_str().len() > 100 {
        bail!("state directory too long for a portable Unix socket; use a shorter path");
    }
    Ok(p)
}
fn call(dir: &Path, id: &str, operation: Operation) -> Result<Response> {
    let mut s = UnixStream::connect(socket(dir, id)?)?;
    s.set_read_timeout(Some(Duration::from_secs(3)))?;
    s.set_write_timeout(Some(Duration::from_secs(3)))?;
    dot_terminal_protocol::write_message(
        &mut s,
        &Request {
            version: VERSION,
            operation,
        },
    )?;
    Ok(dot_terminal_protocol::read_message(&mut s)?)
}
fn spawn(dir: &Path, command: &[String]) -> Result<String> {
    let id = uuid::Uuid::new_v4().simple().to_string();
    socket(dir, &id)?;
    let log = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(dir.join(format!("{id}.log")))?;
    let mut cmd = Command::new(std::env::current_exe()?);
    cmd.arg("--state-dir")
        .arg(dir.canonicalize()?)
        .arg("keeper")
        .arg(&id)
        .arg("--")
        .args(command)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log);
    // Only async-signal-safe libc calls may execute in this post-fork closure.
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() == -1 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    }
    let mut child = cmd.spawn()?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if matches!(call(dir, &id, Operation::Status {}), Ok(Response::Status { session, .. }) if session == id)
        {
            return Ok(id);
        }
        if let Some(status) = child.try_wait()? {
            bail!(
                "keeper exited {status}; inspect {}",
                dir.join(format!("{id}.log")).display()
            );
        }
        thread::sleep(Duration::from_millis(20));
    }
    child.kill()?;
    child.wait()?;
    bail!("keeper startup timed out")
}
struct Controls {
    clock: Instant,
    controller: Controller,
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
}
struct State {
    controls: Mutex<Controls>,
    history: Mutex<History>,
    screen: Mutex<dot_terminal_engine::Screen>,
    exited: AtomicBool,
    output_closed: AtomicBool,
    /// Its own lock: a heartbeat must never wait for, or delay, input.
    presence: Mutex<Presence>,
    started: Instant,
    pid: Option<u32>,
    /// Names this keeper process. Output offsets and resize epochs mean nothing across two.
    incarnation: String,
}
struct SocketGuard(PathBuf);
impl Drop for SocketGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn keeper(dir: &Path, id: &str, command: &[String]) -> Result<()> {
    let path = socket(dir, id)?;
    // No unlink-before-bind: never steal a running keeper's socket.
    let listener = UnixListener::bind(&path)?;
    let _socket_guard = SocketGuard(path.clone());
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    let pair = native_pty_system().openpty(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    })?;
    let mut cmd = CommandBuilder::new(&command[0]);
    cmd.args(&command[1..]);
    cmd.env("TERM", "xterm-256color");
    let mut child = pair.slave.spawn_command(cmd)?;
    drop(pair.slave);
    let mut killer = child.clone_killer();
    let mut reader = pair.master.try_clone_reader()?;
    let writer = pair.master.take_writer()?;
    let state = Arc::new(State {
        controls: Mutex::new(Controls {
            clock: Instant::now(),
            controller: Controller::default(),
            writer,
            master: pair.master,
        }),
        screen: Mutex::new(dot_terminal_engine::Screen::default()),
        history: Mutex::new(History::with_geometry(1024 * 1024, 80, 24)),
        exited: AtomicBool::new(false),
        output_closed: AtomicBool::new(false),
        pid: child.process_id(),
        incarnation: uuid::Uuid::new_v4().simple().to_string(),
        presence: Mutex::new(Presence::default()),
        started: Instant::now(),
    });
    let rstate = state.clone();
    thread::spawn(move || {
        let mut b = [0; 8192];
        loop {
            match reader.read(&mut b) {
                Ok(0) => break,
                Ok(n) => {
                    // One critical section, screen before history — the same order Resize
                    // takes them — so a resize can never land between parsing these bytes
                    // and recording them, which would label them with the wrong grid.
                    let mut screen = rstate.screen.lock().unwrap();
                    let mut history = rstate.history.lock().unwrap();
                    screen.feed(&b[..n]);
                    history.append(&b[..n]);
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
        // EOF is recorded only after all PTY output has reached history.
        rstate.output_closed.store(true, Ordering::Release);
    });
    let child_state = state.clone();
    thread::spawn(move || {
        let _ = child.wait();
        child_state.exited.store(true, Ordering::Release);
    });
    let stop = Arc::new(AtomicBool::new(false));
    let clients = Arc::new(AtomicUsize::new(0));
    while !stop.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((mut stream, _)) => {
                if stop.load(Ordering::Acquire) {
                    break;
                }
                if clients.fetch_add(1, Ordering::AcqRel) >= 32 {
                    clients.fetch_sub(1, Ordering::AcqRel);
                    continue;
                }
                let s = state.clone();
                let stopped = stop.clone();
                let count = clients.clone();
                let id = id.to_owned();
                let wake = path.clone();
                thread::spawn(move || {
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
                    let mut request_stop = false;
                    let response = match dot_terminal_protocol::read_message::<Request>(&mut stream)
                    {
                        Ok(req) => match dot_terminal_protocol::validate(&req) {
                            Err(e) => error(e),
                            Ok(()) => {
                                request_stop = matches!(req.operation, Operation::Stop {});
                                dispatch(&s, &id, req.operation)
                            }
                        },
                        Err(e) => error(e.to_string()),
                    };
                    let _ = dot_terminal_protocol::write_message(&mut stream, &response);
                    if request_stop {
                        stopped.store(true, Ordering::Release);
                        let _ = UnixStream::connect(wake);
                    }
                    count.fetch_sub(1, Ordering::AcqRel);
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10))
            }
            Err(e) => return Err(e.into()),
        }
    }
    let _ = killer.kill();
    Ok(())
}
fn error(message: impl Into<String>) -> Response {
    Response::Error {
        message: message.into(),
    }
}
fn dispatch(s: &State, id: &str, op: Operation) -> Response {
    match op {
        Operation::Status {} => Response::Status {
            version: VERSION,
            session: id.into(),
            pid: s.pid,
            exited: s.exited.load(Ordering::Acquire),
        },
        Operation::Screen {} => {
            let screen = s.screen.lock().unwrap();
            let (cols, rows) = screen.dimensions();
            let (cursor_col, cursor_row) = screen.cursor();
            let response = Response::Screen {
                cols,
                rows,
                lines: screen.lines(),
                cursor_col,
                cursor_row,
                exited: s.exited.load(Ordering::Acquire) && s.output_closed.load(Ordering::Acquire),
            };
            if serde_json::to_vec(&response)
                .is_ok_and(|b| b.len() <= dot_terminal_protocol::MAX_FRAME)
            {
                response
            } else {
                error("screen exceeds frame limit; reduce terminal size")
            }
        }
        Operation::Read { after } => {
            let exited =
                s.exited.load(Ordering::Acquire) && s.output_closed.load(Ordering::Acquire);
            match s.history.lock().unwrap().read(after, 16 * 1024) {
                Ok(c) => Response::Output {
                    start: c.start,
                    next: c.next,
                    gap: c.gap,
                    data: c.data,
                    exited,
                },
                Err(e) => error(e),
            }
        }
        Operation::ReadFrame { after } => {
            let exited =
                s.exited.load(Ordering::Acquire) && s.output_closed.load(Ordering::Acquire);
            match s.history.lock().unwrap().read_frame(after, 16 * 1024) {
                Ok(f) => Response::Frame {
                    start: f.chunk.start,
                    next: f.chunk.next,
                    gap: f.chunk.gap,
                    data: f.chunk.data,
                    exited,
                    cols: f.geometry.cols,
                    rows: f.geometry.rows,
                    geometry_epoch: f.geometry.epoch,
                    incarnation: s.incarnation.clone(),
                },
                Err(e) => error(e),
            }
        }
        Operation::Stop {} => Response::Ack { duplicate: false },
        Operation::Hello { view, label, kind } => {
            let now = s.started.elapsed().as_millis() as u64;
            let mut p = s.presence.lock().unwrap();
            if let Err(e) = p.hello(now, &view, &label, &kind) {
                return error(e);
            }
            let snap = p.snapshot(now);
            Response::Presence {
                views: snap
                    .views
                    .into_iter()
                    .map(|v| dot_terminal_protocol::PresenceView {
                        age_ms: now.saturating_sub(v.seen_ms),
                        view: v.view,
                        label: v.label,
                        kind: v.kind,
                    })
                    .collect(),
                controller: snap.controller,
                controller_known: snap.held,
                controller_idle_ms: snap.controller_idle_ms,
            }
        }
        op => {
            let mut c = match s.controls.try_lock() {
                Ok(c) => c,
                Err(_) => return error("controller busy; input has not been accepted"),
            };
            let now_ms = s.started.elapsed().as_millis() as u64;
            match op {
                Operation::Acquire { takeover } => match c.controller.acquire(takeover) {
                    Ok(g) => {
                        s.presence.lock().unwrap().took(now_ms, None);
                        Response::Lease { generation: g }
                    }
                    Err(e) => error(e),
                },
                Operation::AcquireAs { view, takeover } => {
                    if !dot_terminal_core::valid_view_id(&view) {
                        return error("invalid view id");
                    }
                    match c.controller.acquire(takeover) {
                        Ok(g) => {
                            s.presence.lock().unwrap().took(now_ms, Some(&view));
                            Response::Lease { generation: g }
                        }
                        Err(e) => error(e),
                    }
                }
                Operation::OfferHandoff { generation } => {
                    let ticket = format!(
                        "{}{}",
                        uuid::Uuid::new_v4().simple(),
                        uuid::Uuid::new_v4().simple()
                    );
                    let now = c.clock.elapsed().as_secs();
                    match c.controller.offer_handoff(generation, ticket.clone(), now) {
                        Ok(()) => Response::Handoff {
                            ticket,
                            expires_in: 120,
                        },
                        Err(e) => error(e),
                    }
                }
                Operation::AcceptHandoff { ticket } => {
                    let now = c.clock.elapsed().as_secs();
                    match c.controller.accept_handoff(&ticket, now) {
                        Ok(generation) => Response::Lease { generation },
                        Err(e) => error(e),
                    }
                }
                Operation::CancelHandoff { generation } => {
                    match c.controller.cancel_handoff(generation) {
                        Ok(()) => Response::Ack { duplicate: false },
                        Err(e) => error(e),
                    }
                }
                Operation::CheckControl { generation } => match c.controller.check(generation) {
                    Ok(()) => Response::Ack { duplicate: false },
                    Err(e) => error(e),
                },
                Operation::Release { generation } => match c.controller.release(generation) {
                    Ok(()) => {
                        s.presence.lock().unwrap().cleared();
                        Response::Ack { duplicate: false }
                    }
                    Err(e) => error(e),
                },
                Operation::Resize {
                    generation,
                    cols,
                    rows,
                } => {
                    if let Err(e) = c.controller.check(generation) {
                        return error(e);
                    }
                    if cols > 240 || rows > 100 {
                        return error("screen limit is 240 columns by 100 rows");
                    }
                    // Hold the output history across the ioctl: the reader thread cannot
                    // append while the grid changes, so the mark lands exactly between the
                    // last byte read before the resize and the first read after it. Bytes
                    // the child wrote earlier that are still in the kernel's PTY buffer are
                    // read afterwards and carry the new grid — the keeper cannot see that.
                    let mut screen = s.screen.lock().unwrap();
                    let mut history = s.history.lock().unwrap();
                    match c.master.resize(PtySize {
                        cols,
                        rows,
                        pixel_width: 0,
                        pixel_height: 0,
                    }) {
                        Ok(()) => {
                            screen.resize(cols, rows);
                            history.resize(cols, rows);
                            Response::Ack { duplicate: false }
                        }
                        Err(e) => error(e.to_string()),
                    }
                }
                Operation::Input {
                    generation,
                    sequence,
                    data,
                } => {
                    if s.exited.load(Ordering::Acquire) {
                        return error("session exited");
                    }
                    match c.controller.prepare(generation, sequence, &data) {
                        Err(e) => error(e),
                        Ok(InputDecision::Duplicate) => Response::Ack { duplicate: true },
                        Ok(InputDecision::Write) => {
                            match c.writer.write_all(&data).and_then(|_| c.writer.flush()) {
                                Ok(()) => {
                                    s.presence.lock().unwrap().input(now_ms);
                                    c.controller.written();
                                    Response::Ack { duplicate: false }
                                }
                                Err(_) => {
                                    error("input outcome unknown; do not automatically retry")
                                }
                            }
                        }
                    }
                }
                _ => unreachable!(),
            }
        }
    }
}
fn offer_handoff(dir: &Path, id: &str, node_id: &str, qr: &Path, takeover: bool) -> Result<()> {
    use base64::Engine;
    if !node_id.starts_with("dot:node:v1:")
        || node_id.len() != 76
        || !node_id[12..].bytes().all(|b| b.is_ascii_hexdigit())
    {
        bail!("invalid node identity");
    }
    let generation = match call(dir, id, Operation::Acquire { takeover })? {
        Response::Lease { generation } => generation,
        _ => bail!("detach the current client or explicitly request takeover"),
    };
    let result = (|| -> Result<()> {
        let ticket = match call(dir, id, Operation::OfferHandoff { generation })? {
            Response::Handoff { ticket, .. } => ticket,
            _ => bail!("handoff unavailable"),
        };
        let body = serde_json::json!({"node":node_id,"session":id,"ticket":ticket});
        let capsule = format!(
            "dot-handoff:v1:{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(serde_json::to_vec(&body)?)
        );
        let svg = qrcode::QrCode::new(capsule.as_bytes())?
            .render::<qrcode::render::svg::Color>()
            .min_dimensions(720, 720)
            .build();
        let mut image = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(qr)?;
        image.write_all(svg.as_bytes())?;
        let mut link = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(qr.with_extension("handoff"))?;
        link.write_all(capsule.as_bytes())?;
        println!(
            "Handoff offered for 120 seconds. Scan from an already paired device; keep the QR and sidecar private."
        );
        Ok(())
    })();
    if result.is_err() {
        let _ = call(dir, id, Operation::Release { generation });
    }
    result
}
struct RawMode;
impl Drop for RawMode {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
    }
}
fn attach(dir: &Path, id: &str, takeover: bool, handoff_file: Option<PathBuf>) -> Result<()> {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
    let operation = match handoff_file {
        Some(path) => {
            use base64::Engine;
            let mut data = String::new();
            std::fs::File::open(path)?
                .take(2049)
                .read_to_string(&mut data)?;
            if data.len() > 2048 {
                bail!("handoff size limit");
            }
            let data = data
                .trim()
                .strip_prefix("dot-handoff:v1:")
                .context("not a DOT handoff")?;
            let data = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(data)?;
            let offer: serde_json::Value = serde_json::from_slice(&data)?;
            if offer["session"].as_str() != Some(id) {
                bail!("different session");
            }
            Operation::AcceptHandoff {
                ticket: offer["ticket"].as_str().context("missing ticket")?.into(),
            }
        }
        None => Operation::Acquire { takeover },
    };
    let generation = match call(dir, id, operation)? {
        Response::Lease { generation } => generation,
        r => bail!("cannot take control: {r:?}"),
    };
    let result = (|| -> Result<()> {
        crossterm::terminal::enable_raw_mode()?;
        let _raw = RawMode;
        let (cols, rows) = crossterm::terminal::size()?;
        ensure_ack(call(
            dir,
            id,
            Operation::Resize {
                generation,
                cols,
                rows,
            },
        )?)?;
        let (mut cursor, mut seq) = (0, 0);
        let mut out = std::io::stdout();
        loop {
            match call(dir, id, Operation::Read { after: cursor })? {
                Response::Output {
                    next,
                    data,
                    gap,
                    exited,
                    ..
                } => {
                    if gap {
                        out.write_all(b"\r\n[history truncated: full screen reconstruction not available yet]\r\n")?;
                    }
                    out.write_all(&data)?;
                    out.flush()?;
                    cursor = next;
                    if exited && data.is_empty() {
                        break;
                    }
                }
                r => bail!("read failed: {r:?}"),
            }
            if !event::poll(Duration::from_millis(25))? {
                continue;
            }
            let data = match event::read()? {
                Event::Resize(cols, rows) => {
                    ensure_ack(call(
                        dir,
                        id,
                        Operation::Resize {
                            generation,
                            cols,
                            rows,
                        },
                    )?)?;
                    continue;
                }
                Event::Key(k) if k.kind != KeyEventKind::Release => {
                    if k.modifiers.contains(KeyModifiers::CONTROL) && k.code == KeyCode::Char(']') {
                        break;
                    }
                    match k.code {
                        KeyCode::Char(c)
                            if k.modifiers.contains(KeyModifiers::CONTROL) && c.is_ascii() =>
                        {
                            vec![(c as u8) & 0x1f]
                        }
                        KeyCode::Char(c) => {
                            let mut b = [0; 4];
                            let bytes = c.encode_utf8(&mut b).as_bytes().to_vec();
                            if k.modifiers.contains(KeyModifiers::ALT) {
                                [vec![27], bytes].concat()
                            } else {
                                bytes
                            }
                        }
                        KeyCode::Enter => vec![13],
                        KeyCode::Backspace => vec![127],
                        KeyCode::Tab => vec![9],
                        KeyCode::Esc => vec![27],
                        KeyCode::Up => b"\x1b[A".to_vec(),
                        KeyCode::Down => b"\x1b[B".to_vec(),
                        KeyCode::Right => b"\x1b[C".to_vec(),
                        KeyCode::Left => b"\x1b[D".to_vec(),
                        _ => continue,
                    }
                }
                _ => continue,
            };
            seq += 1;
            ensure_ack(call(
                dir,
                id,
                Operation::Input {
                    generation,
                    sequence: seq,
                    data,
                },
            )?)?;
        }
        Ok(())
    })();
    let _ = call(dir, id, Operation::Release { generation });
    result
}
fn ensure_ack(r: Response) -> Result<()> {
    if let Response::Ack { .. } = r {
        Ok(())
    } else {
        bail!("operation not acknowledged; inspect before retrying: {r:?}")
    }
}
