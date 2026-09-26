//! Compatibility access to an existing owner-operated AXXIS PTY service.
//! This does not adopt processes or claim DOT keeper fencing/replay guarantees.
//! Upstream is loopback only: remote hosts must use an authenticated SSH tunnel.
use crate::{Api, Shared};
use axum::{
    Json,
    extract::{
        Path, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::StatusCode,
    response::{IntoResponse, Response},
};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{os::unix::fs::MetadataExt, path::Path as FilePath, time::Duration};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message as UpMessage, client::IntoClientRequest},
};

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Host {
    pub id: String,
    #[serde(default)]
    pub device_id: Option<String>,
    pub name: String,
    pub url: String,
    pub(crate) token: String,
    #[serde(default)]
    pub(crate) ssh_alias: Option<String>,
    #[serde(default = "default_remote_port")]
    pub(crate) remote_port: u16,
}
fn default_remote_port() -> u16 {
    7431
}
impl Host {
    fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.id.starts_with("external-")
                && self.id.len() <= 32
                && self
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
            "invalid external id"
        );
        anyhow::ensure!(
            crate::devices::valid_name(&self.name),
            "invalid external name"
        );
        anyhow::ensure!(self.remote_port > 0, "invalid remote port");
        if let Some(id) = &self.device_id {
            anyhow::ensure!(
                !id.is_empty()
                    && id.len() <= 32
                    && id
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
                "invalid parent device"
            );
        }
        if let Some(alias) = &self.ssh_alias {
            anyhow::ensure!(
                !alias.is_empty()
                    && alias.len() <= 80
                    && !alias.starts_with('-')
                    && alias
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.'),
                "invalid SSH alias"
            );
        }
        let u: reqwest::Url = self.url.parse()?;
        anyhow::ensure!(
            u.scheme() == "http"
                && u.host_str() == Some("127.0.0.1")
                && u.port().is_some()
                && u.path() == "/"
                && u.query().is_none()
                && u.fragment().is_none()
                && u.username().is_empty()
                && u.password().is_none(),
            "external host requires explicit IPv4 loopback tunnel"
        );
        anyhow::ensure!(
            self.token.len() == 64 && self.token.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid external credential"
        );
        Ok(())
    }
    async fn call(
        &self,
        method: reqwest::Method,
        path: &str,
    ) -> Result<Value, (StatusCode, &'static str)> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(8))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| error())?;
        let r = client
            .request(
                method,
                format!("{}{}", self.url.trim_end_matches('/'), path),
            )
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|_| error())?;
        if !r.status().is_success() {
            return Err((
                StatusCode::BAD_GATEWAY,
                "Existing terminal service refused the request",
            ));
        }
        // Bound decoded bodies even when the upstream sends no Content-Length.
        let mut r = r;
        let mut bytes = Vec::new();
        while let Some(chunk) = r.chunk().await.map_err(|_| error())? {
            if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
                return Err(error());
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| error())
    }
}
fn error() -> (StatusCode, &'static str) {
    (
        StatusCode::BAD_GATEWAY,
        "Existing terminal service unavailable; no operation retried",
    )
}
pub(crate) fn valid_session(id: &str) -> bool {
    (8..=80).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
pub fn load(dir: &FilePath) -> anyhow::Result<Vec<Host>> {
    let path = dir.join("external-hosts.json");
    if !path.exists() {
        return Ok(vec![]);
    }
    let m = std::fs::symlink_metadata(&path)?;
    anyhow::ensure!(
        m.is_file() && m.uid() == unsafe { libc::geteuid() } && m.mode() & 0o077 == 0,
        "external-hosts.json must be owner-private"
    );
    let hosts: Vec<Host> = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    anyhow::ensure!(hosts.len() <= 8, "too many external hosts");
    let mut ids = std::collections::HashSet::new();
    for h in &hosts {
        h.validate()?;
        anyhow::ensure!(ids.insert(&h.id), "duplicate external host");
    }
    Ok(hosts)
}
/// One dedicated tunnel per configured host. Reconnect transport only: never retry input.
/// SSH uses the owner's existing config, known-host checks and identity agent.
/// No credential appears in argv or diagnostics. A competing listener fails closed.
pub fn supervise_tunnels(hosts: &[Host]) {
    for h in hosts {
        let Some(alias) = h.ssh_alias.clone() else {
            continue;
        };
        let port = h
            .url
            .parse::<reqwest::Url>()
            .expect("validated host")
            .port()
            .expect("explicit port");
        let forward = format!("127.0.0.1:{port}:127.0.0.1:{}", h.remote_port);
        tokio::spawn(async move {
            let mut delay = 1;
            loop {
                let started = std::time::Instant::now();
                let child = tokio::process::Command::new("/usr/bin/ssh")
                    .args([
                        "-N",
                        "-T",
                        "-o",
                        "BatchMode=yes",
                        "-o",
                        "ControlMaster=no",
                        "-o",
                        "ControlPath=none",
                        "-o",
                        "ExitOnForwardFailure=yes",
                        "-o",
                        "ConnectTimeout=10",
                        "-o",
                        "ServerAliveInterval=15",
                        "-o",
                        "ServerAliveCountMax=3",
                        "-L",
                        &forward,
                        &alias,
                    ])
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .kill_on_drop(true)
                    .spawn();
                if let Ok(mut child) = child {
                    let _ = child.wait().await;
                }
                if started.elapsed().as_secs() > 60 {
                    delay = 1;
                }
                tokio::time::sleep(Duration::from_secs(delay)).await;
                delay = (delay * 2).min(30);
            }
        });
    }
}
pub async fn catalog(State(app): State<Shared>) -> Api {
    let mut hosts = vec![];
    for h in &app.external {
        let result = h.call(reqwest::Method::GET, "/sessions").await;
        let (state, sessions) = match result {
            Ok(v) if v.is_array() => ("connected", v),
            _ => ("offline", json!([])),
        };
        // A stop started in any view shows in every view until the owner settles it.
        let sessions = match sessions {
            Value::Array(list) => Value::Array(
                list.into_iter()
                    .map(|mut v| {
                        if let Some(sid) = v["session_id"].as_str()
                            && app.stops.stopping(&h.id, sid)
                        {
                            v["stopping"] = json!(true);
                        }
                        v
                    })
                    .collect(),
            ),
            other => other,
        };
        hosts.push(json!({"id":h.id,"name":h.name,"device_id":h.device_id,"state":state,"sessions":sessions,"transport":"axxis-compat","guarantees":{"controller_fencing":false,"checkpoint_replay":false}}));
    }
    Ok(Json(json!({"hosts":hosts})))
}
pub(crate) fn host(app: &Shared, id: &str) -> Result<Host, (StatusCode, &'static str)> {
    app.external
        .iter()
        .find(|h| h.id == id)
        .cloned()
        .ok_or((StatusCode::NOT_FOUND, "Unknown external host"))
}
/// How long a stop waits for the owner to report the process ended before saying it is still running.
/// Agent CLIs shut down gracefully and routinely take longer than a few seconds.
const STOP_BOUND: Duration = if cfg!(test) {
    Duration::from_secs(2)
} else {
    Duration::from_secs(30)
};

/// Stops in flight or recently settled, per (device, session). Every view reads the same record, so a
/// stop started in one view shows as "stopping" in the others and they reconcile on its outcome.
#[derive(Default)]
pub struct Stops(std::sync::Mutex<std::collections::HashMap<(String, String), Value>>);
impl Stops {
    fn get(&self, device: &str, id: &str) -> Option<Value> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&(device.to_owned(), id.to_owned()))
            .cloned()
    }
    fn set(&self, device: &str, id: &str, v: Value) {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert((device.to_owned(), id.to_owned()), v);
    }
    pub(crate) fn stopping(&self, device: &str, id: &str) -> bool {
        self.get(device, id)
            .is_some_and(|v| v["state"] == "stopping")
    }
}

/// Ask the owner to stop the process and answer at once with `stopping`. A background task watches the
/// owner's own state for up to [`STOP_BOUND`] and records `stopped` (the owner reports it exited) or
/// `still_running`. A second request while stopping returns the same record and never sends a second
/// stop. A successful DELETE alone is never reported as the process having ended.
pub async fn stop(State(app): State<Shared>, Path((device, id)): Path<(String, String)>) -> Api {
    if app.attach_only {
        return Err((StatusCode::FORBIDDEN, "Shared view cannot stop sessions"));
    }
    if !valid_session(&id) {
        return Err((StatusCode::BAD_REQUEST, "Invalid session"));
    }
    let h = host(&app, &device)?;
    if let Some(current) = app
        .stops
        .get(&device, &id)
        .filter(|v| v["state"] == "stopping")
    {
        return Ok(Json(current));
    }
    h.call(
        reqwest::Method::DELETE,
        &format!("/sessions/{id}?actor=dot-terminal"),
    )
    .await?;
    let started = chrono_now();
    let record = json!({"session":id,"state":"stopping","requested_at":started});
    app.stops.set(&device, &id, record.clone());
    let app2 = app.clone();
    tokio::spawn(async move {
        let deadline = tokio::time::Instant::now() + STOP_BOUND;
        let outcome = loop {
            if let Ok(v) = h
                .call(reqwest::Method::GET, &format!("/sessions/{id}"))
                .await
                && v["state"] == "exited"
            {
                break json!({"session":id,"state":"stopped","requested_at":started,"owner_state":"exited"});
            }
            if tokio::time::Instant::now() >= deadline {
                break json!({"session":id,"state":"still_running","requested_at":started,
                    "reason":format!("The host still reports this process running {} s after the stop request. Inspect it before stopping again.", STOP_BOUND.as_secs())});
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        };
        app2.stops.set(&device, &id, outcome);
    });
    Ok(Json(record))
}
/// The latest stop record for a session: stopping, stopped, still_running, or none.
pub async fn stop_status(
    State(app): State<Shared>,
    Path((device, id)): Path<(String, String)>,
) -> Api {
    if !valid_session(&id) {
        return Err((StatusCode::BAD_REQUEST, "Invalid session"));
    }
    Ok(Json(
        app.stops
            .get(&device, &id)
            .unwrap_or_else(|| json!({"session":id,"state":"none"})),
    ))
}
fn chrono_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
pub async fn stream(
    State(app): State<Shared>,
    Path((device, id)): Path<(String, String)>,
    ws: WebSocketUpgrade,
) -> Response {
    if !valid_session(&id) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Ok(h) = host(&app, &device) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    ws.max_message_size(64 * 1024)
        .on_upgrade(move |socket| bridge(socket, app, h, id))
}
async fn bridge(mut downstream: WebSocket, app: Shared, h: Host, id: String) {
    // Credential travels in the first frame, not a URL. No upstream connection before auth.
    let first = tokio::time::timeout(Duration::from_secs(5), downstream.recv()).await;
    let authenticated = matches!(first,Ok(Some(Ok(Message::Text(ref t)))) if serde_json::from_str::<Value>(t).ok().is_some_and(|v| v["type"]=="auth" && v["token"].as_str()==Some(app.token.as_str())));
    if !authenticated {
        let _ = downstream.close().await;
        return;
    }
    let url = format!(
        "ws://{}/sessions/{id}/stream",
        h.url.trim_start_matches("http://").trim_end_matches('/')
    );
    let Ok(mut request) = url.into_client_request() else {
        return;
    };
    // AXXIS WebSocket checks its HttpOnly session cookie. Never expose it to the browser.
    let Ok(cookie) = format!("axxisd_session={}", h.token).parse() else {
        return;
    };
    request.headers_mut().insert("Cookie", cookie);
    let upstream = tokio::time::timeout(Duration::from_secs(8), connect_async(request)).await;
    let Ok(Ok((upstream, _))) = upstream else {
        let _ = downstream
            .send(Message::Text("{\"type\":\"upstream_unavailable\"}".into()))
            .await;
        return;
    };
    let (mut tx, mut rx) = upstream.split();
    // This connection's identity for control. It observes until it asks to type.
    let mut random = [0u8; 16];
    if ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut random).is_err() {
        return;
    }
    let me = hex::encode(random);
    let mut last_refusal: Option<std::time::Instant> = None;
    loop {
        tokio::select! {
            from=rx.next()=> {
                let outgoing=match from {
                    Some(Ok(UpMessage::Binary(b)))=>Message::Binary(b),
                    Some(Ok(UpMessage::Text(t)))=>Message::Text(t.to_string().into()),
                    Some(Ok(UpMessage::Ping(b)))=>{if tx.send(UpMessage::Pong(b)).await.is_err(){break;}continue;},
                    Some(Ok(UpMessage::Pong(_)))=>continue,
                    _=>break,
                };
                if !matches!(tokio::time::timeout(Duration::from_secs(5),downstream.send(outgoing)).await,Ok(Ok(()))) {break;}
            },
            from=downstream.recv()=> {
                let (outgoing,gated)=match from {
                    Some(Ok(Message::Binary(b)))=>(UpMessage::Binary(b),true),
                    Some(Ok(Message::Text(t)))=>{
                        let Ok(v)=serde_json::from_str::<Value>(&t) else {break};
                        match v["type"].as_str() {
                            // Control is decided here and never forwarded to the owner.
                            Some("take_control")=>{
                                let reply=match app.control.take(&h.id,&id,&me,v["takeover"].as_bool()==Some(true)){
                                    Ok(g)=>{last_refusal=None;json!({"type":"control","state":"granted","generation":g})},
                                    Err(r)=>json!({"type":"control","state":"refused","reason":r.reason()}),
                                };
                                if downstream.send(Message::Text(reply.to_string().into())).await.is_err(){break;}
                                continue;
                            }
                            Some("release_control")=>{
                                app.control.release(&h.id,&id,&me);
                                if downstream.send(Message::Text(json!({"type":"control","state":"observing"}).to_string().into())).await.is_err(){break;}
                                continue;
                            }
                            Some("resize"|"claim_resize"|"release_resize")=>(UpMessage::Text(t.to_string().into()),true),
                            Some("pause"|"resume"|"ping")=>(UpMessage::Text(t.to_string().into()),false),
                            _=>break,
                        }
                    },
                    Some(Ok(Message::Ping(_)|Message::Pong(_)))=>continue,
                    _=>break,
                };
                if gated {
                    if let Err(r)=app.control.admit(&h.id,&id,&me) {
                        // Dropped, never queued. Say why at once after any change (e.g. taken over),
                        // then at most once a second while the view keeps typing.
                        if last_refusal.is_none_or(|t|t.elapsed()>=Duration::from_secs(1)) {
                            last_refusal=Some(std::time::Instant::now());
                            if downstream.send(Message::Text(json!({"type":"control","state":"observing","reason":r.reason()}).to_string().into())).await.is_err(){break;}
                        }
                        continue;
                    }
                    last_refusal=None;
                }
                if !matches!(tokio::time::timeout(Duration::from_secs(5),tx.send(outgoing)).await,Ok(Ok(()))) {break;}
            }
        }
    }
    app.control.release(&h.id, &id, &me);
    let _ = downstream.close().await;
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn external_hosts_require_explicit_loopback_and_private_credential() {
        let mut h = Host {
            id: "external-core".into(),
            name: "Existing VPS sessions".into(),
            device_id: None,
            url: "http://127.0.0.1:7431/".into(),
            token: "a".repeat(64),
            ssh_alias: None,
            remote_port: 7431,
        };
        assert!(h.validate().is_ok());
        for u in [
            "http://100.64.0.1:7431/",
            "http://localhost:7431/",
            "http://127.0.0.1:7431/?token=x",
            "http://user@127.0.0.1:7431/",
        ] {
            h.url = u.into();
            assert!(h.validate().is_err());
        }
        assert!(valid_session("s_0123456789"));
        assert!(!valid_session("../../secret"));
    }
}
