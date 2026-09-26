#![cfg(unix)]
use dot_terminal_node::{Identity, Peer};
use dot_terminal_protocol::{ServiceRequest, ServiceResponse, read_message, write_message};
use rustls::{
    ClientConfig, ClientConnection, RootCertStore, StreamOwned,
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName},
};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
    os::unix::{fs::PermissionsExt, net::UnixListener},
    process::{Child, Command, Stdio},
    sync::Arc,
    time::Duration,
};
struct Guard(Child);
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn client(
    address: &str,
    server: &Identity,
    identity: Option<&Identity>,
) -> anyhow::Result<ServiceResponse> {
    request(address, server, identity, ServiceRequest::Identity {})
}
fn request(
    address: &str,
    server: &Identity,
    identity: Option<&Identity>,
    message: ServiceRequest,
) -> anyhow::Result<ServiceResponse> {
    let mut roots = RootCertStore::empty();
    roots.add(CertificateDer::from(server.cert.clone()))?;
    let builder = ClientConfig::builder().with_root_certificates(roots);
    let config = match identity {
        Some(i) => builder.with_client_auth_cert(
            vec![CertificateDer::from(i.cert.clone())],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(i.key.clone())),
        )?,
        None => builder.with_no_client_auth(),
    };
    let conn = ClientConnection::new(Arc::new(config), ServerName::try_from("dot.local")?)?;
    let tcp = TcpStream::connect(address)?;
    tcp.set_read_timeout(Some(Duration::from_secs(3)))?;
    tcp.set_write_timeout(Some(Duration::from_secs(3)))?;
    let mut tls = StreamOwned::new(conn, tcp);
    write_message(&mut tls, &message)?;
    Ok(read_message(&mut tls)?)
}
#[test]
fn mutual_identity_and_revocation_are_enforced() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let dir = tempfile::Builder::new()
        .prefix("dot-node-")
        .tempdir_in("/tmp")
        .unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let identity = Identity::generate().unwrap();
    let phone = Identity::generate().unwrap();
    let stranger = Identity::generate().unwrap();
    let mut peers = vec![Peer {
        name: "phone".into(),
        cert: phone.cert.clone(),
        terminal: false,
        workspace: false,
        clipboard_read: false,
        clipboard_write: false,
    }];
    for (name, bytes) in [
        ("identity.json", serde_json::to_vec(&identity).unwrap()),
        ("peers.json", serde_json::to_vec(&peers).unwrap()),
    ] {
        let p = dir.path().join(name);
        std::fs::write(&p, bytes).unwrap();
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let socket = dir.path().join("keeper.sock");
    let _unix = UnixListener::bind(&socket).unwrap();
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600)).unwrap();
    let hub = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let config = dir.path().join("hub.json");
    std::fs::write(
        &config,
        serde_json::to_vec(
            &serde_json::json!({"address":hub.local_addr().unwrap(),"capability":"a".repeat(64)}),
        )
        .unwrap(),
    )
    .unwrap();
    std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o600)).unwrap();
    let child = Command::new(env!("CARGO_BIN_EXE_dot-terminal-node"))
        .arg("--state-dir")
        .arg(dir.path())
        .args(["serve", "--listen", "127.0.0.1:0", "--socket"])
        .arg(socket)
        .arg("--workspace-config")
        .arg(config)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut child = Guard(child);
    let mut line = String::new();
    BufReader::new(child.0.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let address = line.split_whitespace().last().unwrap();
    assert!(
        matches!(client(address,&identity,Some(&phone)).unwrap(),ServiceResponse::Identity{node_id} if node_id==identity.node_id)
    );
    assert!(matches!(
        request(
            address,
            &identity,
            Some(&phone),
            ServiceRequest::ClipboardGet {}
        )
        .unwrap(),
        ServiceResponse::Error { .. }
    ));
    assert!(matches!(
        request(
            address,
            &identity,
            Some(&phone),
            ServiceRequest::ClipboardSet {
                text: "must not overwrite host clipboard".into()
            }
        )
        .unwrap(),
        ServiceResponse::Error { .. }
    ));
    assert!(matches!(
        request(
            address,
            &identity,
            Some(&phone),
            ServiceRequest::Workspace {
                path: "sessions".into(),
                body: None
            }
        )
        .unwrap(),
        ServiceResponse::Error { .. }
    ));
    peers[0].workspace = true;
    std::fs::write(
        dir.path().join("peers.json"),
        serde_json::to_vec(&peers).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        request(
            address,
            &identity,
            Some(&phone),
            ServiceRequest::Workspace {
                path: "vault".into(),
                body: None
            }
        )
        .unwrap(),
        ServiceResponse::Error { .. }
    ));
    let server = std::thread::spawn(move || {
        let (mut socket, _) = hub.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut bytes = Vec::new();
        while !bytes.ends_with(b"\r\n\r\n") && bytes.len() < 2048 {
            let mut byte = [0];
            socket.read_exact(&mut byte).unwrap();
            bytes.push(byte[0]);
        }
        let head = String::from_utf8_lossy(&bytes);
        assert!(head.starts_with("GET /api/sessions HTTP/1.1"));
        assert!(head.contains(&format!("Bearer {}", "a".repeat(64))));
        let body = br#"{"sessions":[]}"#;
        write!(
            socket,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
            body.len()
        )
        .unwrap();
        socket.write_all(body).unwrap();
    });
    assert!(matches!(
        request(
            address,
            &identity,
            Some(&phone),
            ServiceRequest::Workspace {
                path: "sessions".into(),
                body: None
            }
        )
        .unwrap(),
        ServiceResponse::Workspace { status: 200, .. }
    ));
    server.join().unwrap();
    assert!(client(address, &identity, None).is_err());
    assert!(client(address, &identity, Some(&stranger)).is_err());
    assert!(client(address, &stranger, Some(&phone)).is_err());
    std::fs::write(dir.path().join("peers.json"), b"[]").unwrap();
    assert!(client(address, &identity, Some(&phone)).is_err());
}
