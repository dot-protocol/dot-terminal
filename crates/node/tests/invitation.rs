#![cfg(unix)]
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use dot_terminal_node::{Identity, Peer};
use dot_terminal_protocol::{PairInvitation, PairJoin, read_message, write_message};
use ring::{
    rand::SystemRandom,
    signature::{ECDSA_P256_SHA256_ASN1_SIGNING, EcdsaKeyPair},
};
use rustls::{
    ClientConfig, ClientConnection, RootCertStore, StreamOwned,
    pki_types::{CertificateDer, ServerName},
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    net::TcpStream,
    os::unix::fs::PermissionsExt,
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
fn enrollment(approve: bool) {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let dir = tempfile::tempdir().unwrap();
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let host = Identity::generate().unwrap();
    let phone = Identity::generate().unwrap();
    for (name, data) in [
        ("identity.json", serde_json::to_vec(&host).unwrap()),
        ("peers.json", b"[]".to_vec()),
    ] {
        let path = dir.path().join(name);
        fs::write(&path, data).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let qr = dir.path().join("invite.svg");
    let child = Command::new(env!("CARGO_BIN_EXE_dot-terminal-node"))
        .arg("--state-dir")
        .arg(dir.path())
        .args([
            "invite",
            "--listen",
            "127.0.0.1:0",
            "--host",
            "127.0.0.1",
            "--name",
            "phone",
            "--terminal",
            "--ttl",
            "15",
            "--qr",
        ])
        .arg(&qr)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut child = Guard(child);
    let mut output = BufReader::new(child.0.stdout.take().unwrap());
    let mut line = String::new();
    output.read_line(&mut line).unwrap();
    assert!(line.starts_with("Pairing QR ready"));
    let capsule = fs::read_to_string(qr.with_extension("invite")).unwrap();
    let i: PairInvitation = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(capsule.strip_prefix("dot-pair:v1:").unwrap())
            .unwrap(),
    )
    .unwrap();
    let header = format!(
        "DOT-PAIR-V1\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0",
        i.token,
        i.host,
        i.port,
        i.service_port,
        i.expires_at,
        i.terminal,
        i.clipboard_read,
        i.clipboard_write
    );
    let transcript = [header.as_bytes(), &Sha256::digest(&host.cert)].concat();
    let random = SystemRandom::new();
    let key =
        EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &phone.key, &random).unwrap();
    let join = PairJoin {
        version: 1,
        token: i.token,
        certificate: phone.cert.clone(),
        signature: key.sign(&random, &transcript).unwrap().as_ref().to_vec(),
    };
    let code =
        hex::encode(&Sha256::digest([transcript.as_slice(), phone.cert.as_slice()].concat())[..8]);
    let mut roots = RootCertStore::empty();
    roots.add(CertificateDer::from(host.cert)).unwrap();
    let config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let tcp = TcpStream::connect(("127.0.0.1", i.port)).unwrap();
    tcp.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    tcp.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
    let mut tls = StreamOwned::new(
        ClientConnection::new(Arc::new(config), ServerName::try_from("dot.local").unwrap())
            .unwrap(),
        tcp,
    );
    write_message(&mut tls, &join).unwrap();
    line.clear();
    output.read_line(&mut line).unwrap();
    assert!(line.contains(&code));
    // Enrollment must not occur merely because the QR and device signature are valid.
    let peers: Vec<Peer> =
        serde_json::from_slice(&fs::read(dir.path().join("peers.json")).unwrap()).unwrap();
    assert!(peers.is_empty());
    writeln!(
        child.0.stdin.as_mut().unwrap(),
        "{}",
        if approve { &code } else { "no" }
    )
    .unwrap();
    let result: serde_json::Value = read_message(&mut tls).unwrap();
    assert_eq!(result["accepted"], approve);
    let status = child.0.wait().unwrap();
    assert_eq!(status.success(), approve);
    let peers: Vec<Peer> =
        serde_json::from_slice(&fs::read(dir.path().join("peers.json")).unwrap()).unwrap();
    assert_eq!(peers.len(), usize::from(approve));
    if approve {
        assert!(peers[0].terminal);
        assert!(!peers[0].clipboard_read);
        assert_eq!(peers[0].cert, phone.cert);
    }
    assert!(!qr.exists());
    assert!(!qr.with_extension("invite").exists());
    assert!(TcpStream::connect(("127.0.0.1", i.port)).is_err());
}
#[test]
fn enrollment_requires_confirmation_and_closes_one_use_listener() {
    enrollment(true);
}
#[test]
fn declined_enrollment_never_writes_peer_registry() {
    enrollment(false);
}
