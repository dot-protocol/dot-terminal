//! Mutually authenticated service gateway. Identity and grants are independent of network routes.
use anyhow::{Result, bail};
use dot_terminal_protocol::{Operation, ServiceRequest, ServiceResponse};
use rcgen::PublicKeyData;
use rustls::{
    RootCertStore, ServerConfig,
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer},
    server::WebPkiClientVerifier,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub cert: Vec<u8>,
    pub key: Vec<u8>,
    pub node_id: String,
}
impl Identity {
    pub fn generate() -> Result<Self> {
        use sha2::{Digest, Sha256};
        let cert = rcgen::generate_simple_self_signed(vec!["dot.local".into()])?;
        let node_id = format!(
            "dot:node:v1:{}",
            hex::encode(Sha256::digest(cert.signing_key.subject_public_key_info()))
        );
        Ok(Self {
            cert: cert.cert.der().to_vec(),
            key: cert.signing_key.serialize_der(),
            node_id,
        })
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Peer {
    pub name: String,
    pub cert: Vec<u8>,
    pub terminal: bool,
    pub clipboard_read: bool,
    pub clipboard_write: bool,
}
pub fn tls_config(identity: &Identity, peers: &[Peer]) -> Result<Arc<ServerConfig>> {
    if peers.is_empty() {
        bail!("no enrolled peers");
    }
    let mut roots = RootCertStore::empty();
    for peer in peers {
        roots.add(CertificateDer::from(peer.cert.clone()))?;
    }
    let verifier = WebPkiClientVerifier::builder(Arc::new(roots)).build()?;
    let config = ServerConfig::builder()
        .with_client_cert_verifier(verifier)
        .with_single_cert(
            vec![CertificateDer::from(identity.cert.clone())],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(identity.key.clone())),
        )?;
    Ok(Arc::new(config))
}
pub fn authorize(peer: &Peer, request: &ServiceRequest) -> Result<()> {
    dot_terminal_protocol::validate_service(request).map_err(anyhow::Error::msg)?;
    match request {
        ServiceRequest::Identity {} => Ok(()),
        ServiceRequest::Terminal { request }
            if peer.terminal && !matches!(request.operation, Operation::Stop {}) =>
        {
            Ok(())
        }
        ServiceRequest::ClipboardGet {} if peer.clipboard_read => Ok(()),
        ServiceRequest::ClipboardSet { .. } if peer.clipboard_write => Ok(()),
        _ => bail!("service not granted to this device"),
    }
}
pub fn error(message: impl Into<String>) -> ServiceResponse {
    ServiceResponse::Error {
        message: message.into(),
    }
}
/// Absolute socket deadline; byte-by-byte traffic cannot extend the connection lifetime.
pub struct DeadlineStream {
    pub socket: std::net::TcpStream,
    pub deadline: std::time::Instant,
}
impl DeadlineStream {
    fn remaining(&self) -> std::io::Result<std::time::Duration> {
        let remaining = self
            .deadline
            .saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "connection deadline",
            ));
        }
        Ok(remaining)
    }
}
impl std::io::Read for DeadlineStream {
    fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
        self.socket.set_read_timeout(Some(self.remaining()?))?;
        std::io::Read::read(&mut self.socket, b)
    }
}
impl std::io::Write for DeadlineStream {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.socket.set_write_timeout(Some(self.remaining()?))?;
        std::io::Write::write(&mut self.socket, b)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        std::io::Write::flush(&mut self.socket)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dot_terminal_protocol::{Request, VERSION};
    #[test]
    fn terminal_grant_does_not_imply_clipboard_or_shutdown() {
        let peer = Peer {
            name: "phone".into(),
            cert: vec![],
            terminal: true,
            clipboard_read: false,
            clipboard_write: false,
        };
        assert!(authorize(&peer, &ServiceRequest::ClipboardGet {}).is_err());
        assert!(
            authorize(
                &peer,
                &ServiceRequest::ClipboardSet {
                    text: "private".into()
                }
            )
            .is_err()
        );
        assert!(
            authorize(
                &peer,
                &ServiceRequest::Terminal {
                    request: Request {
                        version: VERSION,
                        operation: Operation::Stop {}
                    }
                }
            )
            .is_err()
        );
        assert!(
            authorize(
                &peer,
                &ServiceRequest::Terminal {
                    request: Request {
                        version: VERSION,
                        operation: Operation::Screen {}
                    }
                }
            )
            .is_ok()
        );
    }
    #[test]
    fn identities_are_distinct_and_roundtrip() {
        let a = Identity::generate().unwrap();
        let b = Identity::generate().unwrap();
        assert_ne!(a.node_id, b.node_id);
        let encoded = serde_json::to_vec(&a).unwrap();
        let decoded: Identity = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(a.node_id, decoded.node_id);
    }
}
