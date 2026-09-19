//! Loopback-only desktop projection. Keepers own PTYs independently of this process.
#![cfg(unix)]
mod devices;
mod vault;
use anyhow::{Context, Result, bail};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use clap::Parser;
use dot_terminal_protocol::{Operation, Request, Response as Reply, VERSION};
use ring::rand::{SecureRandom, SystemRandom};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::{fs::MetadataExt, net::UnixStream},
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};
use tower_http::services::ServeDir;

#[derive(Parser)]
struct Args {
    #[arg(long)]
    assets: PathBuf,
    #[arg(long)]
    session_binary: PathBuf,
    #[arg(long)]
    state_dir: Option<PathBuf>,
    #[arg(long)]
    iterm_python: Option<PathBuf>,
    #[arg(long)]
    iterm_bridge: Option<PathBuf>,
    #[arg(long)]
    resource_binary: Option<PathBuf>,
    /// Disable access to the owner vault in isolated UI previews.
    #[arg(long)]
    disable_vault: bool,
    /// Attach to existing sessions without creating keepers from this bundle.
    #[arg(long)]
    attach_only: bool,
    /// Address to serve on. Loopback, or a private overlay (Tailscale/Headscale 100.64.0.0/10)
    /// address so another of the owner's nodes can reach this one. Nothing else is accepted.
    #[arg(long, default_value = "127.0.0.1:0")]
    listen: std::net::SocketAddr,
    /// Read the capability from this private file instead of minting one per start, so a
    /// long-running node keeps the capability its peers were given. 64 hex characters.
    #[arg(long)]
    capability_file: Option<PathBuf>,
    /// What people call this device, and what it is: laptop, server or phone.
    #[arg(long, default_value = "This device")]
    name: String,
    #[arg(long, default_value = "laptop")]
    kind: String,
}
struct Bridge {
    child: Child,
    input: ChildStdin,
    output: mpsc::Receiver<Value>,
}
impl Drop for Bridge {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Bridge {
    fn start(python: PathBuf, script: PathBuf) -> Result<Self> {
        let mut child = Command::new(python)
            .arg("-u")
            .arg(script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let input = child.stdin.take().context("bridge input")?;
        let output = child.stdout.take().context("bridge output")?;
        let (tx, rx) = mpsc::sync_channel(2);
        std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let Ok(line) = line else { break };
                if let Ok(v) = serde_json::from_str(&line)
                    && tx.send(v).is_err()
                {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            input,
            output: rx,
        })
    }
    fn call(&mut self, request: &Value) -> Result<Value> {
        serde_json::to_writer(&mut self.input, request)?;
        self.input.write_all(b"\n")?;
        self.input.flush()?;
        Ok(self.output.recv_timeout(Duration::from_secs(4))?)
    }
}
struct App {
    attach_only: bool,
    name: String,
    kind: String,
    devices: Vec<devices::Device>,
    vault: Mutex<Option<vault::Vault>>,
    resources: Arc<Mutex<Value>>,
    token: String,
    origin: String,
    dir: PathBuf,
    binary: PathBuf,
    bridge: Mutex<Option<Bridge>>,
}
type Shared = Arc<App>;
type Api = Result<Json<Value>, (StatusCode, &'static str)>;
fn failed(_: impl std::fmt::Display) -> (StatusCode, &'static str) {
    (
        StatusCode::BAD_REQUEST,
        "Operation failed; no automatic retry. Check the session before sending input again.",
    )
}
fn session_path(dir: &std::path::Path, id: &str) -> Result<PathBuf> {
    if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!("invalid session")
    }
    Ok(dir.join(format!("{id}.sock")))
}
fn rpc(app: &App, id: &str, operation: Operation) -> Result<Reply> {
    let request = Request {
        version: VERSION,
        operation,
    };
    dot_terminal_protocol::validate(&request).map_err(anyhow::Error::msg)?;
    let mut socket = UnixStream::connect(session_path(&app.dir, id)?)?;
    socket.set_read_timeout(Some(Duration::from_secs(3)))?;
    socket.set_write_timeout(Some(Duration::from_secs(3)))?;
    dot_terminal_protocol::write_message(&mut socket, &request)?;
    Ok(dot_terminal_protocol::read_message(&mut socket)?)
}
async fn boundary(State(app): State<Shared>, req: axum::extract::Request, next: Next) -> Response {
    let host = app.origin.strip_prefix("http://").unwrap();
    if req
        .headers()
        .get(header::HOST)
        .and_then(|x| x.to_str().ok())
        != Some(host)
        || req
            .headers()
            .get(header::ORIGIN)
            .is_some_and(|x| x.as_bytes() != app.origin.as_bytes())
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    if req.uri().path().starts_with("/api/") {
        let expected = format!("Bearer {}", app.token);
        if req
            .headers()
            .get(header::AUTHORIZATION)
            .map(|v| v.as_bytes())
            != Some(expected.as_bytes())
        {
            return StatusCode::UNAUTHORIZED.into_response();
        }
    }
    let mut response = next.run(req).await;
    let h = response.headers_mut();
    h.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    h.insert(header::REFERRER_POLICY, "no-referrer".parse().unwrap());
    h.insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
    h.insert(header::CONTENT_SECURITY_POLICY,"default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'".parse().unwrap());
    response
}
async fn sessions(State(app): State<Shared>) -> Api {
    tokio::task::spawn_blocking(move || {
        let mut sessions = vec![];
        if let Ok(entries) = std::fs::read_dir(&app.dir) {
            for entry in entries.flatten() {
                if let Some(id) = entry.path().file_stem().and_then(|x| x.to_str())
                    && entry.path().extension().is_some_and(|x| x == "sock")
                    && let Ok(r @ Reply::Status { .. }) = rpc(&app, id, Operation::Status {})
                {
                    sessions.push(r);
                }
            }
        }
        Ok(Json(
            json!({"sessions":sessions,"can_create":!app.attach_only}),
        ))
    })
    .await
    .map_err(failed)?
}
async fn create(State(app): State<Shared>) -> Api {
    if app.attach_only {
        return Err((StatusCode::FORBIDDEN, "Shared view cannot create sessions"));
    }
    tokio::task::spawn_blocking(move || {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| {
            if cfg!(target_os = "macos") {
                "/bin/zsh"
            } else {
                "/bin/sh"
            }
            .into()
        });
        let output = Command::new(&app.binary)
            .current_dir(std::env::var_os("HOME").unwrap_or_else(|| "/".into()))
            .arg("--state-dir")
            .arg(&app.dir)
            .args(["new", "--", &shell, "-l"])
            .output()
            .map_err(failed)?;
        if !output.status.success() {
            return Err(failed("spawn"));
        }
        let id = String::from_utf8(output.stdout)
            .map_err(failed)?
            .trim()
            .to_owned();
        session_path(&app.dir, &id).map_err(failed)?;
        Ok(Json(json!({"session":id})))
    })
    .await
    .map_err(failed)?
}
async fn operation(
    State(app): State<Shared>,
    Path(id): Path<String>,
    Json(op): Json<Operation>,
) -> Api {
    if app.attach_only && matches!(op, Operation::Stop {}) {
        return Err((StatusCode::FORBIDDEN, "Shared view cannot stop sessions"));
    }
    tokio::task::spawn_blocking(move || {
        rpc(&app, &id, op)
            .and_then(|x| Ok(Json(serde_json::to_value(x)?)))
            .map_err(failed)
    })
    .await
    .map_err(failed)?
}
/// This device and the owner's other nodes. Capabilities never leave the backend.
async fn device_list(State(app): State<Shared>) -> Api {
    tokio::task::spawn_blocking(move || {
        let mut list = vec![json!({"id":"local","name":app.name,"kind":app.kind,"local":true,"state":"connected","can_create":!app.attach_only})];
        for d in &app.devices {
            let (state, can_create) = match devices::call(d, "GET", "/api/sessions", None) {
                Ok((200, body)) => (
                    "connected",
                    serde_json::from_slice::<Value>(&body)
                        .ok()
                        .and_then(|v| v["can_create"].as_bool())
                        .unwrap_or(false),
                ),
                Ok(_) => ("refused", false),
                Err(_) => ("offline", false),
            };
            list.push(json!({"id":d.id,"name":d.name,"kind":d.kind,"local":false,"state":state,"can_create":can_create&&!app.attach_only}));
        }
        Ok(Json(json!({"devices":list})))
    })
    .await
    .map_err(failed)?
}
fn relay(app: &App, device: &str, method: &str, path: &str, body: Option<Vec<u8>>) -> Api {
    let d = app
        .devices
        .iter()
        .find(|d| d.id == device)
        .ok_or((StatusCode::NOT_FOUND, "Unknown device"))?;
    let (status, bytes) = devices::call(d, method, path, body.as_deref())
        .map_err(|_| (StatusCode::BAD_GATEWAY, "Device unreachable"))?;
    if status != 200 {
        return Err((StatusCode::BAD_GATEWAY, "Device refused the request"));
    }
    serde_json::from_slice(&bytes)
        .map(Json)
        .map_err(|_| (StatusCode::BAD_GATEWAY, "Device sent an invalid reply"))
}
async fn remote_sessions(State(app): State<Shared>, Path(device): Path<String>) -> Api {
    tokio::task::spawn_blocking(move || relay(&app, &device, "GET", "/api/sessions", None))
        .await
        .map_err(failed)?
}
async fn remote_create(State(app): State<Shared>, Path(device): Path<String>) -> Api {
    if app.attach_only {
        return Err((StatusCode::FORBIDDEN, "Shared view cannot create sessions"));
    }
    tokio::task::spawn_blocking(move || {
        relay(&app, &device, "POST", "/api/sessions", Some(b"{}".to_vec()))
    })
    .await
    .map_err(failed)?
}
async fn remote_operation(
    State(app): State<Shared>,
    Path((device, id)): Path<(String, String)>,
    Json(op): Json<Operation>,
) -> Api {
    if app.attach_only && matches!(op, Operation::Stop {}) {
        return Err((StatusCode::FORBIDDEN, "Shared view cannot stop sessions"));
    }
    // The id goes into a remote path: keep it to what a session id can be.
    if id.is_empty() || id.len() > 64 || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err((StatusCode::BAD_REQUEST, "Invalid session"));
    }
    tokio::task::spawn_blocking(move || {
        let body = serde_json::to_vec(&op).map_err(failed)?;
        relay(
            &app,
            &device,
            "POST",
            &format!("/api/sessions/{id}"),
            Some(body),
        )
    })
    .await
    .map_err(failed)?
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ItermRequest {
    action: String,
    id: Option<String>,
    text: Option<String>,
}
async fn iterm(State(app): State<Shared>, Json(request): Json<ItermRequest>) -> Api {
    if !["list", "screen", "input"].contains(&request.action.as_str())
        || request.text.as_ref().is_some_and(|x| x.len() > 16384)
    {
        return Err(failed("invalid bridge request"));
    }
    tokio::task::spawn_blocking(move || {
        let mut guard = app.bridge.lock().map_err(failed)?;
        let bridge = guard.as_mut().ok_or((
            StatusCode::SERVICE_UNAVAILABLE,
            "iTerm bridge unavailable. iTerm must be running with its Python API enabled.",
        ))?;
        match bridge.call(&json!({"action":request.action,"id":request.id,"text":request.text})) {
            Ok(v) => Ok(Json(v)),
            Err(e) => {
                *guard = None;
                Err(failed(e))
            }
        }
    })
    .await
    .map_err(failed)?
}

async fn resource_snapshot(State(app): State<Shared>) -> Api {
    Ok(Json(app.resources.lock().map_err(failed)?.clone()))
}
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum VaultRequest {
    List,
    Put {
        name: String,
        value: String,
    },
    Delete {
        name: String,
    },
    Run {
        command: PathBuf,
        args: Vec<String>,
        secrets: Vec<String>,
    },
}
async fn vault_api(State(app): State<Shared>, Json(request): Json<VaultRequest>) -> Api {
    if app.attach_only {
        return Err((StatusCode::FORBIDDEN, "Shared view cannot access vault"));
    }
    tokio::task::spawn_blocking(move|| {
  let mut guard=app.vault.lock().map_err(failed)?;
  let vault=guard.as_mut().ok_or((StatusCode::SERVICE_UNAVAILABLE,"Vault unavailable: unlock macOS Keychain and close other DOT windows, then reopen DOT."))?;
  match request {
   VaultRequest::List=>Ok(Json(vault.list())),
   VaultRequest::Put{name,value}=>{vault.put(name,value).map_err(failed)?;Ok(Json(vault.list()))},
   VaultRequest::Delete{name}=>{vault.delete(&name).map_err(failed)?;Ok(Json(vault.list()))},
   VaultRequest::Run{command,args,secrets}=>{
    if !command.is_absolute()||args.len()>128{return Err(failed("invalid executable"))}
    let env=vault.release(&secrets,&command,&args).map_err(failed)?;
    let output=Command::new(&app.binary).arg("--state-dir").arg(&app.dir).args(["new","--"]).arg(&command).args(&args).envs(env).output().map_err(failed)?;
    if !output.status.success(){return Err(failed("launch failed after disclosure"))}
    let id=String::from_utf8(output.stdout).map_err(failed)?.trim().to_owned();session_path(&app.dir,&id).map_err(failed)?;
    vault.launched(&id).map_err(failed)?;Ok(Json(json!({"session":id})))
   }
  }
 }).await.map_err(failed)?
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    if !args.assets.join("index.html").is_file() {
        bail!("Build desktop assets before starting")
    }
    let dir = args.state_dir.unwrap_or_else(|| {
        PathBuf::from(format!("/tmp/dot-terminal-{}", unsafe { libc::geteuid() }))
    });
    // The session CLI creates and validates the private runtime directory.
    let status = Command::new(&args.session_binary)
        .arg("--state-dir")
        .arg(&dir)
        .arg("list")
        .stdout(Stdio::null())
        .status()?;
    if !status.success() {
        bail!("Session runtime unavailable")
    }
    let m = std::fs::symlink_metadata(&dir)?;
    if !m.is_dir() || m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o077 != 0 {
        bail!("Unsafe runtime directory")
    }
    let mut random = [0; 32];
    SystemRandom::new()
        .fill(&mut random)
        .map_err(|_| anyhow::anyhow!("randomness unavailable"))?;
    if !devices::private_overlay(args.listen.ip()) {
        bail!("--listen accepts only loopback or a private overlay (100.64.0.0/10) address")
    }
    if !devices::valid_name(&args.name) || !devices::KINDS.contains(&args.kind.as_str()) {
        bail!("invalid --name or --kind")
    }
    let token = match &args.capability_file {
        None => hex::encode(random),
        Some(path) => {
            let m = std::fs::symlink_metadata(path)?;
            if !m.is_file() || m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o077 != 0 {
                bail!("capability file must be a private file owned by this user")
            }
            let t = std::fs::read_to_string(path)?.trim().to_owned();
            if t.len() != 64 || !t.bytes().all(|b| b.is_ascii_hexdigit()) {
                bail!("capability file must hold 64 hex characters")
            }
            t
        }
    };
    let remote_devices = devices::load(&dir)?;
    let listener = tokio::net::TcpListener::bind(args.listen).await?;
    let origin = format!("http://{}", listener.local_addr()?);
    let bridge = match (args.iterm_python, args.iterm_bridge) {
        (Some(p), Some(s)) => Bridge::start(p, s).ok(),
        _ => None,
    };
    let resources = Arc::new(Mutex::new(json!({"state":"starting"})));
    if let Some(binary) = args.resource_binary {
        let sink = resources.clone();
        std::thread::spawn(move || {
            let Ok(mut child) = Command::new(binary)
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
            else {
                return;
            };
            if let Some(out) = child.stdout.take() {
                for line in BufReader::new(out).lines() {
                    let Ok(line) = line else { break };
                    if let Ok(value) = serde_json::from_str(&line)
                        && let Ok(mut slot) = sink.lock()
                    {
                        *slot = value;
                    }
                }
            }
            let _ = child.wait();
        });
    }
    let vault = Mutex::new(if args.disable_vault || args.attach_only {
        None
    } else {
        vault::Vault::open_default().ok()
    });
    let app = Arc::new(App {
        attach_only: args.attach_only,
        vault,
        resources,
        name: args.name,
        kind: args.kind,
        devices: remote_devices,
        token,
        origin,
        dir,
        binary: args.session_binary,
        bridge: Mutex::new(bridge),
    });
    let router = Router::new()
        .route("/api/sessions", get(sessions).post(create))
        .route("/api/sessions/{id}", post(operation))
        .route("/api/devices", get(device_list))
        .route(
            "/api/devices/{device}/sessions",
            get(remote_sessions).post(remote_create),
        )
        .route(
            "/api/devices/{device}/sessions/{id}",
            post(remote_operation),
        )
        .route("/api/iterm", post(iterm))
        .route("/api/resources", get(resource_snapshot))
        .route("/api/vault", post(vault_api))
        .fallback_service(ServeDir::new(args.assets))
        .layer(DefaultBodyLimit::max(64 * 1024))
        .layer(middleware::from_fn_with_state(app.clone(), boundary))
        .with_state(app.clone());
    // Private parent pipe only: never send this capability to analytics or logs.
    println!("{}/#{}", app.origin, app.token);
    axum::serve(listener, router).await?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn shared_view_rejects_session_creation_before_spawning() {
        let app = Arc::new(App {
            name: "Test".into(),
            kind: "laptop".into(),
            devices: vec![],
            attach_only: true,
            vault: Mutex::new(None),
            resources: Arc::new(Mutex::new(json!({}))),
            token: String::new(),
            origin: String::new(),
            dir: PathBuf::new(),
            binary: PathBuf::from("/not-an-executable"),
            bridge: Mutex::new(None),
        });
        assert_eq!(
            create(State(app.clone())).await.unwrap_err().0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            operation(
                State(app.clone()),
                Path("no-session".into()),
                Json(Operation::Stop {})
            )
            .await
            .unwrap_err()
            .0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            vault_api(
                State(app),
                Json(VaultRequest::Run {
                    command: PathBuf::from("/not-an-executable"),
                    args: vec![],
                    secrets: vec![]
                })
            )
            .await
            .unwrap_err()
            .0,
            StatusCode::FORBIDDEN
        );
    }
    #[test]
    fn rejects_socket_traversal() {
        assert!(session_path(std::path::Path::new("/tmp"), "../../other").is_err());
        assert!(session_path(std::path::Path::new("/tmp"), &"a".repeat(32)).is_ok());
    }
    #[tokio::test]
    async fn loopback_boundary_rejects_foreign_origins_and_missing_authority() {
        use tower::ServiceExt;
        let app = Arc::new(App {
            name: "Test".into(),
            kind: "laptop".into(),
            devices: vec![],
            attach_only: false,
            vault: Mutex::new(None),
            resources: Arc::new(Mutex::new(json!({}))),
            token: "test-capability".into(),
            origin: "http://127.0.0.1:12345".into(),
            dir: PathBuf::new(),
            binary: PathBuf::new(),
            bridge: Mutex::new(None),
        });
        let router = Router::new()
            .route("/api/test", get(|| async { "ok" }))
            .layer(middleware::from_fn_with_state(app, boundary));
        for (host, origin, token, expected) in [
            ("127.0.0.1:12345", None, None, StatusCode::UNAUTHORIZED),
            (
                "127.0.0.1:12345",
                Some("https://attacker.example"),
                Some("Bearer test-capability"),
                StatusCode::FORBIDDEN,
            ),
            (
                "attacker.example:12345",
                None,
                Some("Bearer test-capability"),
                StatusCode::FORBIDDEN,
            ),
            (
                "127.0.0.1:12345",
                Some("http://127.0.0.1:12345"),
                Some("Bearer test-capability"),
                StatusCode::OK,
            ),
        ] {
            let mut request = axum::http::Request::builder()
                .uri("/api/test")
                .header("host", host);
            if let Some(o) = origin {
                request = request.header("origin", o);
            }
            if let Some(t) = token {
                request = request.header("authorization", t);
            }
            let response = router
                .clone()
                .oneshot(request.body(axum::body::Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), expected);
            if expected == StatusCode::OK {
                assert_eq!(response.headers()["cache-control"], "no-store");
                assert!(
                    response.headers()["content-security-policy"]
                        .to_str()
                        .unwrap()
                        .contains("frame-ancestors 'none'")
                );
            }
        }
    }
}
