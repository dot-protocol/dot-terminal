#![cfg(unix)]
use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use dot_terminal_core::{Controller, History, InputDecision};
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
        Commands::Attach { id, takeover } => attach(&dir, &id, takeover)?,
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
    pid: Option<u32>,
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
            controller: Controller::default(),
            writer,
            master: pair.master,
        }),
        screen: Mutex::new(dot_terminal_engine::Screen::default()),
        history: Mutex::new(History::new(1024 * 1024)),
        exited: AtomicBool::new(false),
        output_closed: AtomicBool::new(false),
        pid: child.process_id(),
    });
    let rstate = state.clone();
    thread::spawn(move || {
        let mut b = [0; 8192];
        loop {
            match reader.read(&mut b) {
                Ok(0) => break,
                Ok(n) => {
                    rstate.screen.lock().unwrap().feed(&b[..n]);
                    rstate.history.lock().unwrap().append(&b[..n]);
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
        Operation::Stop {} => Response::Ack { duplicate: false },
        op => {
            let mut c = match s.controls.try_lock() {
                Ok(c) => c,
                Err(_) => return error("controller busy; input has not been accepted"),
            };
            match op {
                Operation::Acquire { takeover } => match c.controller.acquire(takeover) {
                    Ok(g) => Response::Lease { generation: g },
                    Err(e) => error(e),
                },
                Operation::Release { generation } => match c.controller.release(generation) {
                    Ok(()) => Response::Ack { duplicate: false },
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
                    let mut screen = s.screen.lock().unwrap();
                    match c.master.resize(PtySize {
                        cols,
                        rows,
                        pixel_width: 0,
                        pixel_height: 0,
                    }) {
                        Ok(()) => {
                            screen.resize(cols, rows);
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
struct RawMode;
impl Drop for RawMode {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
    }
}
fn attach(dir: &Path, id: &str, takeover: bool) -> Result<()> {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
    let generation = match call(dir, id, Operation::Acquire { takeover })? {
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
