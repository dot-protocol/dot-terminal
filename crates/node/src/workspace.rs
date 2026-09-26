//! A paired workspace is a distinct grant, not an expansion of a one-terminal grant.
//! Only session catalog / keeper operations reach a fixed loopback hub. Owner APIs
//! (vault, iTerm, resources, automation, view actions) are deliberately unreachable.
use anyhow::{Context, Result, bail};
use dot_terminal_protocol::{Operation, Request, VERSION};
use serde::Deserialize;
use serde_json::Value;
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    time::Duration,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hub {
    address: SocketAddr,
    capability: String,
}
impl Hub {
    pub fn validate(&self) -> Result<()> {
        if !self.address.ip().is_loopback()
            || self.capability.len() != 64
            || !self.capability.bytes().all(|b| b.is_ascii_hexdigit())
        {
            bail!("workspace requires a loopback hub and valid capability");
        }
        Ok(())
    }
    pub fn call(&self, path: &str, body: Option<&Value>) -> Result<(u16, Value)> {
        self.validate()?;
        allowed(path, body)?;
        let mut stream = TcpStream::connect_timeout(&self.address, Duration::from_secs(2))?;
        stream.set_read_timeout(Some(Duration::from_secs(8)))?;
        stream.set_write_timeout(Some(Duration::from_secs(3)))?;
        let data = body
            .map(serde_json::to_vec)
            .transpose()?
            .unwrap_or_default();
        let method = if body.is_some() { "POST" } else { "GET" };
        write!(
            stream,
            "{method} /api/{path} HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            self.address,
            self.capability,
            data.len()
        )?;
        stream.write_all(&data)?;
        let mut raw = Vec::new();
        stream.take(2 * 1024 * 1024 + 1).read_to_end(&mut raw)?;
        if raw.len() > 2 * 1024 * 1024 {
            bail!("workspace response limit");
        }
        let split = raw
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .context("invalid hub response")?;
        let head = std::str::from_utf8(&raw[..split])?;
        let status = head
            .split_whitespace()
            .nth(1)
            .context("missing status")?
            .parse()?;
        if head.to_ascii_lowercase().contains("transfer-encoding") {
            bail!("unsupported hub encoding");
        }
        let value = serde_json::from_slice(&raw[split + 4..])
            .unwrap_or_else(|_| serde_json::json!({"error":"workspace request failed"}));
        Ok((status, value))
    }
}
fn identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
fn allowed(path: &str, body: Option<&Value>) -> Result<()> {
    if path == "devices" && body.is_none() {
        return Ok(());
    }
    if path == "session-labels" {
        if let Some(v) = body {
            let o = v.as_object().context("label object required")?;
            if o.len() != 3 || !["device", "id", "name"].iter().all(|k| o.contains_key(*k)) {
                bail!("invalid label fields");
            }
            if !["device", "id"]
                .iter()
                .all(|k| o[*k].as_str().is_some_and(identifier))
                || !o["name"].as_str().is_some_and(|n| {
                    !n.trim().is_empty() && n.len() <= 160 && !n.chars().any(char::is_control)
                })
            {
                bail!("invalid label");
            }
        }
        return Ok(());
    }
    if path == "external" && body.is_none() {
        return Ok(());
    }
    if path == "workspace-tabs" {
        if let Some(v) = body {
            let o = v.as_object().context("tab object required")?;
            if o.len() != 3
                || !matches!(v["action"].as_str(), Some("open" | "close"))
                || !v["device"].as_str().is_some_and(identifier)
                || !v["id"].as_str().is_some_and(identifier)
            {
                bail!("invalid tab operation");
            }
        }
        return Ok(());
    }
    let parts: Vec<_> = path.split('/').collect();
    if let ["external", device, id, "view"] = parts.as_slice() {
        let v = body.context("view operation required")?;
        if identifier(device)
            && identifier(id)
            && matches!(
                v["op"].as_str(),
                Some("open" | "read" | "send" | "close" | "take_control" | "release_control")
            )
            && serde_json::to_vec(v)?.len() <= 128 * 1024
        {
            return Ok(());
        }
        bail!("invalid view operation");
    }
    let route = match parts.as_slice() {
        ["sessions", rest @ ..] => rest,
        ["devices", device, "sessions", rest @ ..] if identifier(device) => rest,
        _ => bail!("workspace route is not granted"),
    };
    match route {
        [] if body.is_none() => Ok(()),
        [] if body.is_some_and(|v| v.as_object().is_some_and(|o| o.is_empty())) => Ok(()),
        [id] if identifier(id) => {
            let value = body.context("keeper request required")?;
            if serde_json::to_vec(value)?.len() > 128 * 1024 {
                bail!("workspace request limit");
            }
            let operation: Operation = serde_json::from_value(value.clone())?;
            if matches!(operation, Operation::Stop {}) {
                bail!("stopping a session is not granted");
            }
            dot_terminal_protocol::validate(&Request {
                version: VERSION,
                operation,
            })
            .map_err(anyhow::Error::msg)
        }
        _ => bail!("workspace route is not granted"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn labels_cannot_reach_owner_services_or_write_arbitrary_fields() {
        assert!(allowed("session-labels", None).is_ok());
        assert!(allowed("session-labels", Some(&json!({"device":"local","id":"abcdef0123456789abcdef0123456789","name":"Research"}))).is_ok());
        for body in [
            json!({"device":"../escape","id":"abc","name":"Test"}),
            json!({"device":"local","id":"abc","name":"Test","command":"sh"}),
            json!({"device":"local","id":"abc","name":"\u{001b}"}),
        ] {
            assert!(allowed("session-labels", Some(&body)).is_err());
        }
    }
    #[test]
    fn a_phone_can_ask_for_and_give_back_control_of_an_existing_session() {
        for op in [
            json!({"op":"take_control","view":"v","takeover":false}),
            json!({"op":"release_control","view":"v"}),
        ] {
            assert!(
                allowed("external/external-core/s_test/view", Some(&op)).is_ok(),
                "{op} refused"
            );
        }
        assert!(
            allowed(
                "external/external-core/s_test/view",
                Some(&json!({"op":"stop","view":"v"}))
            )
            .is_err()
        );
    }
    #[test]
    fn native_stream_grant_does_not_allow_process_stop_or_owner_routes() {
        assert!(allowed("external", None).is_ok());
        assert!(
            allowed(
                "external/external-core/s_test/view",
                Some(&json!({"op":"open"}))
            )
            .is_ok()
        );
        assert!(
            allowed(
                "workspace-tabs",
                Some(&json!({"action":"close","device":"external-core","id":"s_test"}))
            )
            .is_ok()
        );
        for path in [
            "external/external-core/s_test/stop",
            "external/../s_test/view",
            "external/external-core/s_test/view?x=1",
        ] {
            assert!(allowed(path, Some(&json!({"op":"open"}))).is_err());
        }
        assert!(
            allowed(
                "external/external-core/s_test/view",
                Some(&json!({"op":"stop"}))
            )
            .is_err()
        );
    }
    #[test]
    fn authority_is_bounded_to_catalog_and_keeper() {
        for path in ["devices", "sessions", "devices/core/sessions"] {
            assert!(allowed(path, None).is_ok());
        }
        assert!(allowed("sessions", Some(&json!({}))).is_ok());
        assert!(allowed("devices/core/sessions/abc", Some(&json!({"type":"screen"}))).is_ok());
        for path in [
            "vault",
            "resources",
            "devices/local/resources",
            "devices/core/resources",
            "iterm",
            "view-actions",
            "ui-state",
            "sessions/a/events",
            "devices/../sessions",
            "sessions/a?x=y",
            "sessions/%61",
        ] {
            assert!(allowed(path, None).is_err(), "{path}");
        }
        assert!(allowed("sessions/abc", Some(&json!({"type":"stop"}))).is_err());
        assert!(allowed("sessions", Some(&json!({"command":"sh"}))).is_err());
        assert!(
            Hub {
                address: "192.168.1.2:80".parse().unwrap(),
                capability: "a".repeat(64)
            }
            .validate()
            .is_err()
        );
    }
}
