//! Strict local control protocol. No OS or async-runtime dependencies.
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};

pub const VERSION: u16 = 1;
pub const MAX_FRAME: usize = 256 * 1024;
pub const MAX_INPUT: usize = 16 * 1024;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub version: u16,
    pub operation: Operation,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    Status {},
    Screen {},
    Read {
        after: u64,
    },
    /// `Read`, with the grid its bytes were produced under. A keeper that predates this
    /// answers `Error`; the caller then uses `Read` and samples `Screen` as before.
    ReadFrame {
        after: u64,
    },
    Acquire {
        takeover: bool,
    },
    Input {
        generation: u64,
        sequence: u64,
        data: Vec<u8>,
    },
    Resize {
        generation: u64,
        cols: u16,
        rows: u16,
    },
    Release {
        generation: u64,
    },
    OfferHandoff {
        generation: u64,
    },
    AcceptHandoff {
        ticket: String,
    },
    CancelHandoff {
        generation: u64,
    },
    CheckControl {
        generation: u64,
    },
    Stop {},
    /// A view's heartbeat; the answer is who is here and who is typing. For people, not authority.
    Hello {
        view: String,
        label: String,
        kind: String,
    },
    /// Keep this connection open and receive a `Frame` (exactly what `ReadFrame` returns) each time
    /// output or a resize lands after `after`, an empty frame as a heartbeat when nothing has for a
    /// while, and a final frame with `exited` before the keeper closes it. A keeper that predates this
    /// answers `Error`; the caller then polls `ReadFrame` as before.
    Subscribe {
        after: u64,
    },
    /// `Acquire`, naming the view that takes control so other views can show it.
    AcquireAs {
        view: String,
        takeover: bool,
    },
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PresenceView {
    pub view: String,
    pub label: String,
    pub kind: String,
    pub age_ms: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Response {
    Presence {
        views: Vec<PresenceView>,
        controller: Option<String>,
        controller_known: bool,
        controller_idle_ms: Option<u64>,
    },
    Screen {
        cols: u16,
        rows: u16,
        lines: Vec<String>,
        cursor_col: u16,
        cursor_row: u16,
        exited: bool,
    },
    Status {
        version: u16,
        session: String,
        pid: Option<u32>,
        exited: bool,
    },
    Output {
        start: u64,
        next: u64,
        gap: bool,
        data: Vec<u8>,
        exited: bool,
    },
    /// Output that never spans a resize: every byte of `data` was produced on the
    /// `cols` x `rows` grid. Apply the grid, then parse the bytes. `geometry_epoch` counts
    /// this stream's resizes; `incarnation` names the keeper process that owns the offsets.
    Frame {
        start: u64,
        next: u64,
        gap: bool,
        data: Vec<u8>,
        exited: bool,
        cols: u16,
        rows: u16,
        geometry_epoch: u64,
        incarnation: String,
    },
    Handoff {
        ticket: String,
        expires_in: u64,
    },
    Lease {
        generation: u64,
    },
    Ack {
        duplicate: bool,
    },
    Error {
        message: String,
    },
}

/// Application services are independent of IP addressing and the local keeper protocol.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "service", rename_all = "snake_case", deny_unknown_fields)]
pub enum ServiceRequest {
    Terminal {
        request: Request,
    },
    ClipboardGet {},
    ClipboardSet {
        text: String,
    },
    Identity {},
    Workspace {
        path: String,
        body: Option<serde_json::Value>,
    },
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "service", rename_all = "snake_case", deny_unknown_fields)]
pub enum ServiceResponse {
    Terminal {
        response: Response,
    },
    Clipboard {
        text: String,
    },
    Identity {
        node_id: String,
    },
    Workspace {
        status: u16,
        body: serde_json::Value,
    },
    Ack {},
    Error {
        message: String,
    },
}
pub const MAX_CLIPBOARD: usize = 64 * 1024;
pub fn validate_service(req: &ServiceRequest) -> Result<(), &'static str> {
    match req {
        ServiceRequest::Terminal { request } => validate(request),
        ServiceRequest::ClipboardSet { text } if text.len() > MAX_CLIPBOARD => {
            Err("clipboard limit exceeded")
        }
        _ => Ok(()),
    }
}

pub fn read_message<T: for<'a> Deserialize<'a>>(r: &mut impl Read) -> io::Result<T> {
    let mut size = [0; 4];
    r.read_exact(&mut size)?;
    let len = u32::from_be_bytes(size) as usize;
    if len > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame limit exceeded",
        ));
    }
    let mut bytes = vec![0; len];
    r.read_exact(&mut bytes)?;
    serde_json::from_slice(&bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}
pub fn write_message<T: Serialize>(w: &mut impl Write, value: &T) -> io::Result<()> {
    let data = serde_json::to_vec(value).map_err(io::Error::other)?;
    if data.len() > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "frame limit exceeded",
        ));
    }
    w.write_all(&(data.len() as u32).to_be_bytes())?;
    w.write_all(&data)?;
    w.flush()
}
pub fn validate(req: &Request) -> Result<(), &'static str> {
    if req.version != VERSION {
        return Err("unsupported protocol version");
    }
    match &req.operation {
        Operation::AcceptHandoff { ticket }
            if ticket.len() != 64 || !ticket.bytes().all(|b| b.is_ascii_hexdigit()) =>
        {
            Err("invalid handoff ticket")
        }
        Operation::Input { data, .. } if data.len() > MAX_INPUT => Err("input limit exceeded"),
        Operation::Resize { cols, rows, .. }
            if *cols == 0 || *rows == 0 || *cols > 240 || *rows > 100 =>
        {
            Err("invalid dimensions")
        }
        _ => Ok(()),
    }
}

/// Short-lived QR bootstrap. Tokens are secrets and must not be logged.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairInvitation {
    pub version: u16,
    pub host: String,
    pub port: u16,
    pub service_port: u16,
    pub certificate: String,
    pub token: String,
    pub expires_at: u64,
    pub terminal: bool,
    pub clipboard_read: bool,
    pub clipboard_write: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairJoin {
    pub version: u16,
    pub token: String,
    pub certificate: Vec<u8>,
    pub signature: Vec<u8>,
}
pub fn validate_invitation(i: &PairInvitation) -> Result<(), &'static str> {
    if i.version != 1
        || i.host.is_empty()
        || i.host.len() > 253
        || !i
            .host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-:".contains(&b))
        || i.port == 0
        || i.service_port == 0
        || i.certificate.len() > 8192
        || i.token.len() != 64
        || !i.token.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("invalid pairing invitation");
    }
    Ok(())
}
pub fn validate_join(i: &PairJoin) -> Result<(), &'static str> {
    if i.version != 1
        || i.certificate.is_empty()
        || i.certificate.len() > 8192
        || i.signature.is_empty()
        || i.signature.len() > 128
        || i.token.len() != 64
        || !i.token.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("invalid enrollment request");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn service_requests_are_strict_and_clipboard_limit_is_in_bytes() {
        assert!(
            serde_json::from_str::<ServiceRequest>(r#"{"service":"clipboard_get","hidden":true}"#)
                .is_err()
        );
        assert!(serde_json::from_str::<ServiceRequest>(r#"{"service":"execute"}"#).is_err());
        assert!(
            validate_service(&ServiceRequest::ClipboardSet {
                text: "é".repeat(MAX_CLIPBOARD / 2)
            })
            .is_ok()
        );
        assert!(
            validate_service(&ServiceRequest::ClipboardSet {
                text: "é".repeat(MAX_CLIPBOARD / 2 + 1)
            })
            .is_err()
        );
    }
    #[test]
    fn rejects_length_before_payload_allocation() {
        assert!(read_message::<Request>(&mut &u32::MAX.to_be_bytes()[..]).is_err());
    }
    #[test]
    fn rejects_unknown_fields_and_versions() {
        assert!(
            serde_json::from_str::<Request>(
                r#"{"version":1,"operation":{"type":"status","surprise":true}}"#
            )
            .is_err()
        );
        assert!(
            validate(&Request {
                version: 2,
                operation: Operation::Status {}
            })
            .is_err()
        );
    }
    #[test]
    fn roundtrip_binary_input() {
        let r = Request {
            version: VERSION,
            operation: Operation::Input {
                generation: 9,
                sequence: 1,
                data: vec![0, 255, 27],
            },
        };
        let mut b = Vec::new();
        write_message(&mut b, &r).unwrap();
        let out: Request = read_message(&mut b.as_slice()).unwrap();
        assert!(matches!(out.operation,Operation::Input{data,..} if data==vec![0,255,27]));
    }
    #[test]
    fn rejects_zero_size() {
        assert!(
            validate(&Request {
                version: 1,
                operation: Operation::Resize {
                    generation: 1,
                    cols: 0,
                    rows: 24
                }
            })
            .is_err()
        );
    }

    /// Views decide "this keeper predates read_frame" from this exact serde wording. Pin it.
    #[test]
    fn an_unknown_operation_is_reported_as_an_unknown_variant() {
        let e = serde_json::from_str::<Operation>(r#"{"type":"not_an_operation"}"#).unwrap_err();
        assert!(e.to_string().contains("unknown variant"), "{e}");
    }
}
