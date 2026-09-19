//! A short-lived pinned TLS bootstrap. Possession of a QR alone never grants services.
use super::{load, save};
use anyhow::{Context, Result, bail};
use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use clap::Args;
use dot_terminal_node::{Identity, Peer, tls_config};
use dot_terminal_protocol::{
    PairInvitation, PairJoin, validate_invitation, validate_join, write_message,
};
use ring::rand::{SecureRandom, SystemRandom};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    net::{SocketAddr, TcpListener},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use subtle::ConstantTimeEq;
#[derive(Args)]
pub struct Invite {
    #[arg(long)]
    pub listen: SocketAddr,
    #[arg(long)]
    pub host: String,
    #[arg(long, default_value_t = 17843)]
    pub service_port: u16,
    #[arg(long)]
    pub name: String,
    #[arg(long)]
    pub qr: PathBuf,
    #[arg(long, default_value_t = 120)]
    pub ttl: u64,
    #[arg(long)]
    pub terminal: bool,
    #[arg(long)]
    pub clipboard_read: bool,
    #[arg(long)]
    pub clipboard_write: bool,
}
struct Artifacts(Vec<PathBuf>);
impl Drop for Artifacts {
    fn drop(&mut self) {
        for p in &self.0 {
            let _ = fs::remove_file(p);
        }
    }
}
fn artifact(paths: &mut Artifacts, path: PathBuf, bytes: &[u8]) -> Result<()> {
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)?;
    paths.0.push(path);
    f.write_all(bytes)?;
    Ok(())
}
fn proof_message(i: &PairInvitation, server_cert: &[u8]) -> Vec<u8> {
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
    [header.as_bytes(), &Sha256::digest(server_cert)].concat()
}
fn check_join(join: &PairJoin, invitation: &PairInvitation, server_cert: &[u8]) -> Result<String> {
    validate_join(join).map_err(anyhow::Error::msg)?;
    if !bool::from(join.token.as_bytes().ct_eq(invitation.token.as_bytes())) {
        bail!("invitation mismatch");
    }
    let cert = CertificateDer::from(join.certificate.as_slice());
    let entity = webpki::EndEntityCert::try_from(&cert)?;
    entity.verify_signature(
        webpki::ring::ECDSA_P256_SHA256,
        &proof_message(invitation, server_cert),
        &join.signature,
    )?;
    let code = Sha256::digest(
        [
            proof_message(invitation, server_cert).as_slice(),
            join.certificate.as_slice(),
        ]
        .concat(),
    );
    Ok(hex::encode(&code[..8]))
}
pub fn run(state: &Path, args: Invite) -> Result<()> {
    if !(15..=300).contains(&args.ttl)
        || args.name.is_empty()
        || args.name.len() > 64
        || !args
            .name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        bail!("invalid invitation settings");
    }
    let identity: Identity = load(&state.join("identity.json"))?;
    let mut token = [0u8; 32];
    SystemRandom::new()
        .fill(&mut token)
        .map_err(|_| anyhow::anyhow!("random source unavailable"))?;
    let listener = TcpListener::bind(args.listen)?;
    listener.set_nonblocking(true)?;
    let invitation = PairInvitation {
        version: 1,
        host: args.host,
        port: listener.local_addr()?.port(),
        service_port: args.service_port,
        certificate: STANDARD.encode(&identity.cert),
        token: hex::encode(token),
        expires_at: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() + args.ttl,
        terminal: args.terminal,
        clipboard_read: args.clipboard_read,
        clipboard_write: args.clipboard_write,
    };
    validate_invitation(&invitation).map_err(anyhow::Error::msg)?;
    let capsule = format!(
        "dot-pair:v1:{}",
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&invitation)?)
    );
    let code = qrcode::QrCode::new(capsule.as_bytes())?;
    let svg = code
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(720, 720)
        .build();
    let mut artifacts = Artifacts(Vec::new());
    artifact(
        &mut artifacts,
        args.qr.with_extension("invite"),
        capsule.as_bytes(),
    )?;
    artifact(&mut artifacts, args.qr.clone(), svg.as_bytes())?;
    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(
            vec![CertificateDer::from(identity.cert.clone())],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(identity.key.clone())),
        )?;
    let config = Arc::new(config);
    let deadline = Instant::now() + Duration::from_secs(args.ttl);
    println!(
        "Pairing QR ready. Expires in {} seconds. Keep this invitation private.",
        args.ttl
    );
    std::io::stdout().flush()?;
    let mut attempts = 0;
    while Instant::now() < deadline && attempts < 16 {
        let (socket, _) = match listener.accept() {
            Ok(v) => v,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(50));
                continue;
            }
            Err(e) => return Err(e.into()),
        };
        socket.set_nonblocking(false)?;
        attempts += 1;
        let remaining = deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_secs(4));
        if remaining.is_zero() {
            break;
        }
        socket.set_read_timeout(Some(remaining))?;
        socket.set_write_timeout(Some(remaining))?;
        let mut tls = rustls::StreamOwned::new(
            rustls::ServerConnection::new(config.clone())?,
            dot_terminal_node::DeadlineStream {
                socket,
                deadline: Instant::now() + remaining,
            },
        );
        // A bounded bootstrap frame; this endpoint can never execute terminal or clipboard requests.
        let join: PairJoin = match read_join(&mut tls) {
            Ok(v) => v,
            Err(_) => continue,
        };
        tls.sock.deadline = deadline;
        let code = match check_join(&join, &invitation, &identity.cert) {
            Ok(v) => v,
            Err(_) => continue,
        };
        println!(
            "Compare with the phone: {code}. Type that exact code here to approve '{}' (terminal={}, clipboard-read={}, clipboard-write={}).",
            args.name, args.terminal, args.clipboard_read, args.clipboard_write
        );
        std::io::stdout().flush()?;
        let (tx, rx) = mpsc::sync_channel(1);
        thread::spawn(move || {
            let mut line = String::new();
            let result = std::io::stdin().read_line(&mut line);
            let _ = tx.send((result.is_ok(), line));
        });
        let approved = rx
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .ok()
            .is_some_and(|(ok, line)| ok && line.trim() == code);
        if !approved || Instant::now() >= deadline {
            let _ = write_message(&mut tls, &serde_json::json!({"accepted":false}));
            bail!("pairing not approved or expired");
        }
        let _lock = super::registry_lock(state)?;
        let path = state.join("peers.json");
        let mut peers: Vec<Peer> = load(&path)?;
        if let Some(old) = peers.iter().find(|p| p.name == args.name)
            && old.cert != join.certificate
        {
            bail!("peer name belongs to another key; revoke it explicitly first");
        }
        if peers
            .iter()
            .any(|p| p.cert == join.certificate && p.name != args.name)
        {
            bail!("device already enrolled under another name");
        }
        peers.retain(|p| p.name != args.name);
        if peers.len() >= 32 {
            bail!("peer limit");
        }
        peers.push(Peer {
            name: args.name,
            cert: join.certificate,
            terminal: args.terminal,
            clipboard_read: args.clipboard_read,
            clipboard_write: args.clipboard_write,
        });
        tls_config(&identity, &peers)?;
        save(&path, &peers)?;
        write_message(&mut tls, &serde_json::json!({"accepted":true}))?;
        println!("Device paired. Invitation consumed; bootstrap listener closed.");
        return Ok(());
    }
    bail!("invitation expired or attempt limit reached")
}
fn read_join(r: &mut impl Read) -> Result<PairJoin> {
    let mut size = [0; 4];
    r.read_exact(&mut size)?;
    let n = u32::from_be_bytes(size) as usize;
    if n > 48 * 1024 {
        bail!("pairing frame limit");
    }
    let mut b = vec![0; n];
    r.read_exact(&mut b)?;
    serde_json::from_slice(&b).context("invalid pairing request")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Identity, PairInvitation, PairJoin) {
        let server = Identity::generate().unwrap();
        let client = Identity::generate().unwrap();
        let i = PairInvitation {
            version: 1,
            host: "127.0.0.1".into(),
            port: 17844,
            service_port: 17843,
            certificate: STANDARD.encode(&server.cert),
            token: "a".repeat(64),
            expires_at: 123,
            terminal: true,
            clipboard_read: false,
            clipboard_write: false,
        };
        let random = SystemRandom::new();
        let key = ring::signature::EcdsaKeyPair::from_pkcs8(
            &ring::signature::ECDSA_P256_SHA256_ASN1_SIGNING,
            &client.key,
            &random,
        )
        .unwrap();
        let signature = key
            .sign(&random, &proof_message(&i, &server.cert))
            .unwrap()
            .as_ref()
            .to_vec();
        let j = PairJoin {
            version: 1,
            token: i.token.clone(),
            certificate: client.cert,
            signature,
        };
        (server, i, j)
    }
    #[test]
    fn signature_binds_key_permissions_and_invitation() {
        let (server, mut i, mut j) = fixture();
        assert_eq!(check_join(&j, &i, &server.cert).unwrap().len(), 16);
        i.clipboard_read = true;
        assert!(check_join(&j, &i, &server.cert).is_err());
        i.clipboard_read = false;
        i.service_port += 1;
        assert!(check_join(&j, &i, &server.cert).is_err());
        i.service_port -= 1;
        j.token = "b".repeat(64);
        assert!(check_join(&j, &i, &server.cert).is_err());
        j.token = i.token.clone();
        j.certificate = Identity::generate().unwrap().cert;
        assert!(check_join(&j, &i, &server.cert).is_err());
    }
    #[test]
    fn bootstrap_rejects_oversized_frame_without_payload() {
        assert!(read_join(&mut &u32::MAX.to_be_bytes()[..]).is_err());
    }
}
