#![cfg(unix)]
mod pairing;
mod workspace;
use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use dot_terminal_node::{Identity, Peer, authorize, error, tls_config};
use dot_terminal_protocol::{
    MAX_CLIPBOARD, ServiceRequest, ServiceResponse, read_message, write_message,
};
use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    os::unix::{
        fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
        net::UnixStream,
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};
#[derive(Parser)]
struct Cli {
    #[arg(long)]
    state_dir: PathBuf,
    #[command(subcommand)]
    command: Commands,
}
#[derive(Subcommand)]
enum Commands {
    /// Create a short-lived QR invitation; enrollment requires a matching code on both devices.
    Invite(pairing::Invite),
    Init,
    Pair {
        #[arg(long)]
        cert: PathBuf,
        #[arg(long)]
        name: String,
        #[arg(long)]
        terminal: bool,
        #[arg(long)]
        workspace: bool,
        #[arg(long)]
        clipboard_read: bool,
        #[arg(long)]
        clipboard_write: bool,
    },
    Revoke {
        #[arg(long)]
        name: String,
    },
    Serve {
        #[arg(long)]
        listen: std::net::SocketAddr,
        #[arg(long)]
        socket: Option<PathBuf>,
        /// Private loopback hub configuration; never sent to the paired device.
        #[arg(long)]
        workspace_config: Option<PathBuf>,
    },
}
fn private(path: &Path, directory: bool) -> Result<()> {
    let m = fs::symlink_metadata(path)?;
    if m.file_type().is_symlink()
        || m.uid() != unsafe { libc::geteuid() }
        || m.mode() & 0o077 != 0
        || (directory && !m.is_dir())
    {
        bail!("private owned path required");
    }
    Ok(())
}
fn load<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    private(path, false)?;
    if !fs::symlink_metadata(path)?.is_file() {
        bail!("regular state file required");
    }
    let mut b = Vec::new();
    fs::File::open(path)?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut b)?;
    if b.len() > 1024 * 1024 {
        bail!("state limit");
    }
    Ok(serde_json::from_slice(&b)?)
}
fn save<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    let temp = path.with_extension("pending");
    let mut f = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&temp)?;
    serde_json::to_writer(&mut f, value)?;
    f.sync_all()?;
    fs::rename(temp, path)?;
    Ok(())
}
struct RegistryLock(fs::File);
impl Drop for RegistryLock {
    fn drop(&mut self) {
        use std::os::fd::AsRawFd;
        unsafe {
            libc::flock(self.0.as_raw_fd(), libc::LOCK_UN);
        }
    }
}
fn registry_lock(state: &Path) -> Result<RegistryLock> {
    use std::os::fd::AsRawFd;
    let path = state.join("registry.lock");
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)?;
    private(&path, false)?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        bail!("another enrollment change is in progress; retry");
    }
    Ok(RegistryLock(file))
}
fn main() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let cli = Cli::parse();
    if !cli.state_dir.exists() {
        fs::DirBuilder::new().mode(0o700).create(&cli.state_dir)?;
    }
    private(&cli.state_dir, true)?;
    let _lock = if matches!(
        &cli.command,
        Commands::Init | Commands::Pair { .. } | Commands::Revoke { .. }
    ) {
        Some(registry_lock(&cli.state_dir)?)
    } else {
        None
    };
    let identity_path = cli.state_dir.join("identity.json");
    let peers_path = cli.state_dir.join("peers.json");
    match cli.command {
        Commands::Invite(args) => pairing::run(&cli.state_dir, args)?,
        Commands::Init => {
            if identity_path.exists() {
                let i: Identity = load(&identity_path)?;
                println!("{}", i.node_id);
                return Ok(());
            }
            let i = Identity::generate()?;
            save(&identity_path, &i)?;
            if !peers_path.exists() {
                save(&peers_path, &Vec::<Peer>::new())?;
            }
            let mut cert = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(cli.state_dir.join("device-cert.der"))?;
            cert.write_all(&i.cert)?;
            println!("{}", i.node_id);
        }
        Commands::Pair {
            cert,
            name,
            terminal,
            workspace,
            clipboard_read,
            clipboard_write,
        } => {
            if name.is_empty()
                || name.len() > 64
                || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            {
                bail!("use a short alphanumeric peer name");
            }
            let mut peers: Vec<Peer> = load(&peers_path)?;
            if peers.iter().any(|p| p.name == name) {
                bail!("peer name exists; revoke before replacing its identity");
            }
            let mut cert_bytes = Vec::new();
            fs::File::open(cert)?
                .take(8193)
                .read_to_end(&mut cert_bytes)?;
            let cert = cert_bytes;
            if cert.len() > 8192 {
                bail!("certificate limit");
            }
            if peers.len() >= 32 {
                bail!("peer limit");
            }
            peers.push(Peer {
                name,
                cert,
                terminal,
                workspace,
                clipboard_read,
                clipboard_write,
            });
            let identity: Identity = load(&identity_path)?;
            tls_config(&identity, &peers)?;
            save(&peers_path, &peers)?;
            println!("Device enrolled with explicit service grants.");
        }
        Commands::Revoke { name } => {
            let mut p: Vec<Peer> = load(&peers_path)?;
            p.retain(|p| p.name != name);
            save(&peers_path, &p)?;
            println!("Device revoked for subsequent requests.");
        }
        Commands::Serve {
            listen,
            socket,
            workspace_config,
        } => {
            let hub = workspace_config
                .as_deref()
                .map(load::<workspace::Hub>)
                .transpose()?
                .map(Arc::new);
            if let Some(hub) = &hub {
                hub.validate()?;
            }
            if let Some(socket) = &socket {
                private(socket, false)?;
                private(socket.parent().context("socket parent")?, true)?;
            }
            if socket.is_none() && hub.is_none() {
                bail!("configure a terminal socket or workspace hub");
            }
            let identity: Identity = load(&identity_path)?;
            let listener = TcpListener::bind(listen)?;
            let count = Arc::new(AtomicUsize::new(0));
            println!(
                "DOT authenticated node listening on {}",
                listener.local_addr()?
            );
            for stream in listener.incoming() {
                let stream = stream?;
                if count.fetch_add(1, Ordering::AcqRel) >= 16 {
                    count.fetch_sub(1, Ordering::AcqRel);
                    continue;
                }
                let peers: Vec<Peer> = match load(&peers_path) {
                    Ok(p) => p,
                    Err(_) => {
                        count.fetch_sub(1, Ordering::AcqRel);
                        continue;
                    }
                };
                let config = match tls_config(&identity, &peers) {
                    Ok(c) => c,
                    Err(_) => {
                        count.fetch_sub(1, Ordering::AcqRel);
                        continue;
                    }
                };
                let count = count.clone();
                let socket = socket.clone();
                let registry = peers_path.clone();
                let node_id = identity.node_id.clone();
                let hub = hub.clone();
                thread::spawn(move || {
                    let _ = serve(
                        stream,
                        config,
                        &registry,
                        socket.as_deref(),
                        &node_id,
                        hub.as_deref(),
                    );
                    count.fetch_sub(1, Ordering::AcqRel);
                });
            }
        }
    }
    Ok(())
}
fn serve(
    stream: TcpStream,
    config: Arc<rustls::ServerConfig>,
    registry: &Path,
    socket: Option<&Path>,
    node_id: &str,
    hub: Option<&workspace::Hub>,
) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(4)))?;
    stream.set_write_timeout(Some(Duration::from_secs(4)))?;
    let conn = rustls::ServerConnection::new(config)?;
    let mut tls = rustls::StreamOwned::new(
        conn,
        dot_terminal_node::DeadlineStream {
            socket: stream,
            deadline: std::time::Instant::now() + Duration::from_secs(4),
        },
    );
    while tls.conn.is_handshaking() {
        tls.conn
            .complete_io(&mut tls.sock)
            .context("TLS handshake")?;
    }
    let cert = tls
        .conn
        .peer_certificates()
        .and_then(|v| v.first())
        .context("client identity required")?
        .to_vec();
    let request: ServiceRequest = read_message(&mut tls).context("service frame")?;
    let peers: Vec<Peer> = load(registry)?;
    let peer = peers
        .iter()
        .find(|p| p.cert == cert)
        .context("device revoked or not pinned")?;
    if matches!(&request, ServiceRequest::Workspace { .. }) {
        tls.sock.deadline = std::time::Instant::now() + Duration::from_secs(10);
    }
    let response = match authorize(peer, &request) {
        Err(e) => error(e.to_string()),
        Ok(()) => match request {
            ServiceRequest::Workspace { path, body } => match hub {
                Some(hub) => match hub.call(&path, body.as_ref()) {
                    Ok((status, body)) => ServiceResponse::Workspace { status, body },
                    Err(e) => error(e.to_string()),
                },
                None => error("workspace service is not configured"),
            },
            ServiceRequest::Identity {} => ServiceResponse::Identity {
                node_id: node_id.into(),
            },
            ServiceRequest::Terminal { request } => {
                let Some(socket) = socket else {
                    write_message(
                        &mut tls,
                        &error("single terminal service is not configured"),
                    )?;
                    return Ok(());
                };
                let mut stream = UnixStream::connect(socket)?;
                stream.set_read_timeout(Some(Duration::from_secs(3)))?;
                stream.set_write_timeout(Some(Duration::from_secs(3)))?;
                write_message(&mut stream, &request)?;
                ServiceResponse::Terminal {
                    response: read_message(&mut stream)?,
                }
            }
            ServiceRequest::ClipboardGet {} => match clipboard_get() {
                Ok(text) => ServiceResponse::Clipboard { text },
                Err(e) => error(e.to_string()),
            },
            ServiceRequest::ClipboardSet { text } => match clipboard_set(&text) {
                Ok(()) => ServiceResponse::Ack {},
                Err(e) => error(e.to_string()),
            },
        },
    };
    write_message(&mut tls, &response)?;
    tls.conn.send_close_notify();
    let _ = tls.flush();
    Ok(())
}
fn clipboard_get() -> Result<String> {
    if !cfg!(target_os = "macos") {
        bail!("OS clipboard adapter unavailable on this platform");
    }
    let mut child = Command::new("/usr/bin/pbpaste")
        .env("LC_ALL", "en_US.UTF-8")
        .stdout(Stdio::piped())
        .spawn()?;
    let mut bytes = Vec::new();
    child
        .stdout
        .take()
        .context("clipboard pipe")?
        .take((MAX_CLIPBOARD + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_CLIPBOARD {
        let _ = child.kill();
        let _ = child.wait();
        bail!("clipboard limit exceeded");
    }
    if !child.wait()?.success() {
        bail!("clipboard unavailable");
    }
    Ok(String::from_utf8(bytes)?)
}
fn clipboard_set(text: &str) -> Result<()> {
    if !cfg!(target_os = "macos") {
        bail!("OS clipboard adapter unavailable on this platform");
    }
    let mut child = Command::new("/usr/bin/pbcopy")
        .env("LC_ALL", "en_US.UTF-8")
        .stdin(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .context("clipboard pipe")?
        .write_all(text.as_bytes())?;
    if !child.wait()?.success() {
        bail!("clipboard unavailable");
    }
    Ok(())
}
