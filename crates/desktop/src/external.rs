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
    pub name: String,
    pub url: String,
    token: String,
    #[serde(default)]
    ssh_alias: Option<String>,
    #[serde(default = "default_remote_port")]
    remote_port: u16,
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
fn valid_session(id: &str) -> bool {
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
        hosts.push(json!({"id":h.id,"name":h.name,"state":state,"sessions":sessions,"transport":"axxis-compat","guarantees":{"controller_fencing":false,"checkpoint_replay":false}}));
    }
    Ok(Json(json!({"hosts":hosts})))
}
fn host(app: &Shared, id: &str) -> Result<Host, (StatusCode, &'static str)> {
    app.external
        .iter()
        .find(|h| h.id == id)
        .cloned()
        .ok_or((StatusCode::NOT_FOUND, "Unknown external host"))
}
pub async fn stop(State(app): State<Shared>, Path((device, id)): Path<(String, String)>) -> Api {
    if app.attach_only {
        return Err((StatusCode::FORBIDDEN, "Shared view cannot stop sessions"));
    }
    if !valid_session(&id) {
        return Err((StatusCode::BAD_REQUEST, "Invalid session"));
    }
    let h = host(&app, &device)?;
    let _ = h
        .call(
            reqwest::Method::DELETE,
            &format!("/sessions/{id}?actor=dot-terminal"),
        )
        .await?;
    // A successful DELETE is not proof of process exit. Require the owner's state.
    for _ in 0..12 {
        let v = h
            .call(reqwest::Method::GET, &format!("/sessions/{id}"))
            .await?;
        if matches!(v["state"].as_str(), Some("exited")) {
            return Ok(Json(
                json!({"stopped":true,"session":id,"state":v["state"]}),
            ));
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    Err((
        StatusCode::CONFLICT,
        "Stop requested but exit is not confirmed; inspect the session before retrying",
    ))
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
                let outgoing=match from {
                    Some(Ok(Message::Binary(b)))=>UpMessage::Binary(b),
                    Some(Ok(Message::Text(t)))=>{
                        let Ok(v)=serde_json::from_str::<Value>(&t) else {break};
                        if !matches!(v["type"].as_str(),Some("resize"|"pause"|"resume"|"ping")){break;}
                        UpMessage::Text(t.to_string().into())
                    },
                    Some(Ok(Message::Ping(_)|Message::Pong(_)))=>continue,
                    _=>break,
                };
                if !matches!(tokio::time::timeout(Duration::from_secs(5),tx.send(outgoing)).await,Ok(Ok(()))) {break;}
            }
        }
    }
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
