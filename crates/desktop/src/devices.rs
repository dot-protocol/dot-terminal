//! Other DOT nodes this backend can reach, and a minimal client to reach them.
//!
//! The browser only ever talks to its own backend. The backend holds each remote node's
//! capability and relays session calls, so a remote capability never reaches page JavaScript.
//!
//! Transport is plain HTTP and is accepted ONLY to loopback or to the CGNAT range 100.64.0.0/10
//! used by WireGuard overlays such as Tailscale/Headscale, where the overlay provides encryption
//! and peer authentication. Anything else is refused: this client must never carry a capability
//! over an open network.
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::{
    io::{Read, Write},
    net::{IpAddr, SocketAddr, TcpStream},
    os::unix::fs::MetadataExt,
    path::Path,
    time::Duration,
};

const MAX_DEVICES: usize = 16;
const MAX_RESPONSE: usize = 2 * 1024 * 1024;
pub const KINDS: [&str; 3] = ["laptop", "server", "phone"];

/// Loopback, or an overlay address. Used for both `--listen` and remote device URLs.
pub fn private_overlay(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback() || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xc0) == 64)
        }
        IpAddr::V6(v6) => v6.is_loopback(),
    }
}

#[derive(Clone, Debug)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub addr: SocketAddr,
    capability: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileDevice {
    id: String,
    name: String,
    kind: String,
    url: String,
    capability: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    schema: String,
    devices: Vec<FileDevice>,
}

pub fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.chars().count() <= 40 && !name.chars().any(char::is_control)
}
fn valid_id(id: &str) -> bool {
    (1..=32).contains(&id.len())
        && id != "local"
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

pub fn parse(text: &str) -> Result<Vec<Device>> {
    let file: File = serde_json::from_str(text).context("devices file is not valid")?;
    if file.schema != "dot.devices.v1" || file.devices.len() > MAX_DEVICES {
        bail!("unsupported devices file")
    }
    let mut out: Vec<Device> = vec![];
    for d in file.devices {
        let addr: SocketAddr = d
            .url
            .strip_prefix("http://")
            .map(|x| x.trim_end_matches('/'))
            .and_then(|x| x.parse().ok())
            .context("device url must be http://<ip>:<port>")?;
        if !private_overlay(addr.ip()) {
            bail!(
                "device {} is not on loopback or a private overlay address",
                d.id
            )
        }
        if !valid_id(&d.id) || out.iter().any(|x| x.id == d.id) || !valid_name(&d.name) {
            bail!("invalid device id or name")
        }
        if !KINDS.contains(&d.kind.as_str()) {
            bail!("invalid device kind")
        }
        if d.capability.len() != 64 || !d.capability.bytes().all(|b| b.is_ascii_hexdigit()) {
            bail!("invalid device capability")
        }
        out.push(Device {
            id: d.id,
            name: d.name,
            kind: d.kind,
            addr,
            capability: d.capability,
        });
    }
    Ok(out)
}

/// `<state-dir>/devices.json`. It holds capabilities, so it must be the owner's and private.
pub fn load(dir: &Path) -> Result<Vec<Device>> {
    let path = dir.join("devices.json");
    let Ok(meta) = std::fs::symlink_metadata(&path) else {
        return Ok(vec![]);
    };
    if !meta.is_file() || meta.uid() != unsafe { libc::geteuid() } || meta.mode() & 0o077 != 0 {
        bail!("devices.json must be a private file owned by this user")
    }
    parse(&std::fs::read_to_string(path)?)
}

/// One JSON call to a remote node's backend. Returns (status, body).
pub fn call(
    device: &Device,
    method: &str,
    path: &str,
    body: Option<&[u8]>,
) -> Result<(u16, Vec<u8>)> {
    if !path.starts_with("/api/") || path.bytes().any(|b| b <= b' ' || b == 0x7f) {
        bail!("invalid remote path")
    }
    let mut s = TcpStream::connect_timeout(&device.addr, Duration::from_secs(3))?;
    s.set_read_timeout(Some(Duration::from_secs(5)))?;
    s.set_write_timeout(Some(Duration::from_secs(5)))?;
    let body = body.unwrap_or_default();
    let head = format!(
        "{method} {path} HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        device.addr,
        device.capability,
        body.len()
    );
    s.write_all(head.as_bytes())?;
    s.write_all(body)?;
    let mut raw = Vec::new();
    s.take(MAX_RESPONSE as u64 + 1).read_to_end(&mut raw)?;
    if raw.len() > MAX_RESPONSE {
        bail!("remote response too large")
    }
    parse_response(&raw)
}

fn parse_response(raw: &[u8]) -> Result<(u16, Vec<u8>)> {
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .context("malformed remote response")?;
    let head = std::str::from_utf8(&raw[..split]).context("malformed remote response")?;
    let status: u16 = head
        .split_whitespace()
        .nth(1)
        .and_then(|x| x.parse().ok())
        .context("malformed remote status")?;
    if head
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        bail!("unsupported remote encoding")
    }
    Ok((status, raw[split + 4..].to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;
    const CAP: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
    fn file(url: &str, id: &str, kind: &str) -> String {
        format!(
            r#"{{"schema":"dot.devices.v1","devices":[{{"id":"{id}","name":"core VPS","kind":"{kind}","url":"{url}","capability":"{CAP}"}}]}}"#
        )
    }
    #[test]
    fn only_loopback_and_overlay_addresses_may_carry_a_capability() {
        assert!(parse(&file("http://100.108.136.47:7420", "core", "server")).is_ok());
        assert!(parse(&file("http://127.0.0.1:7420/", "core", "server")).is_ok());
        for open in [
            "http://192.168.1.9:7420",
            "http://8.8.8.8:80",
            "http://100.128.0.1:1",
            "https://100.108.136.47:7420",
            "http://core.example:7420",
        ] {
            assert!(parse(&file(open, "core", "server")).is_err(), "{open}");
        }
    }
    #[test]
    fn ids_kinds_and_unknown_fields_are_strict() {
        assert!(
            parse(&file("http://127.0.0.1:1", "local", "server")).is_err(),
            "local is reserved"
        );
        assert!(parse(&file("http://127.0.0.1:1", "Core VPS", "server")).is_err());
        assert!(parse(&file("http://127.0.0.1:1", "core", "mainframe")).is_err());
        assert!(parse(r#"{"schema":"dot.devices.v1","devices":[],"note":1}"#).is_err());
        assert!(parse(r#"{"schema":"other","devices":[]}"#).is_err());
    }
    #[test]
    fn a_shared_or_foreign_devices_file_is_refused() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        assert!(
            load(dir.path()).unwrap().is_empty(),
            "no file means no remote devices"
        );
        let path = dir.path().join("devices.json");
        std::fs::write(&path, file("http://127.0.0.1:1", "core", "server")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(load(dir.path()).is_err());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(load(dir.path()).unwrap()[0].id, "core");
    }
    #[test]
    fn responses_are_parsed_without_trusting_their_shape() {
        assert_eq!(
            parse_response(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\n{}").unwrap(),
            (200, b"{}".to_vec())
        );
        assert!(parse_response(b"garbage").is_err());
        assert!(
            parse_response(
                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\n{}\r\n0\r\n\r\n"
            )
            .is_err()
        );
    }
    #[test]
    fn a_remote_call_reaches_a_real_server_with_host_and_bearer() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut buf = [0; 2048];
            let n = s.read(&mut buf).unwrap();
            s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\n\r\n{\"ok\":true}")
                .unwrap();
            String::from_utf8_lossy(&buf[..n]).to_string()
        });
        let device = parse(&file(&format!("http://{addr}"), "core", "server"))
            .unwrap()
            .remove(0);
        let (status, body) = call(&device, "GET", "/api/sessions", None).unwrap();
        let seen = server.join().unwrap();
        assert_eq!(
            (status, body.as_slice()),
            (200, br#"{"ok":true}"#.as_slice())
        );
        assert!(seen.contains(&format!("Host: {addr}")) && seen.contains(&format!("Bearer {CAP}")));
        assert!(call(&device, "GET", "/etc/passwd", None).is_err());
        assert!(call(&device, "GET", "/api/x y", None).is_err());
    }
}
