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
    Stop {},
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Response {
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
    Terminal { request: Request },
    ClipboardGet {},
    ClipboardSet { text: String },
    Identity {},
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "service", rename_all = "snake_case", deny_unknown_fields)]
pub enum ServiceResponse {
    Terminal { response: Response },
    Clipboard { text: String },
    Identity { node_id: String },
    Ack {},
    Error { message: String },
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
        Operation::Input { data, .. } if data.len() > MAX_INPUT => Err("input limit exceeded"),
        Operation::Resize { cols, rows, .. }
            if *cols == 0 || *rows == 0 || *cols > 240 || *rows > 100 =>
        {
            Err("invalid dimensions")
        }
        _ => Ok(()),
    }
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
}
