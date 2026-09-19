//! Small native executable: validates the actual portable crates on a device.
//! It needs no filesystem, network, microphone, account, or model access.
use dot_terminal_core::{Controller, History, InputDecision};
use dot_terminal_protocol::{Operation, Request, VERSION, read_message, validate, write_message};

fn main() {
    let mut passed = Vec::new();
    let mut record = |name: &str, condition: bool| {
        assert!(condition, "device check failed: {name}");
        passed.push(name.to_owned());
    };
    let request = Request {
        version: VERSION,
        operation: Operation::Input {
            generation: 1,
            sequence: 1,
            data: vec![0, 27, 255],
        },
    };
    let mut frame = Vec::new();
    write_message(&mut frame, &request).expect("encode");
    let decoded: Request = read_message(&mut frame.as_slice()).expect("decode");
    record(
        "binary_input_roundtrip",
        matches!(decoded.operation, Operation::Input { data, .. } if data == [0,27,255]),
    );
    record(
        "oversized_frame_rejected",
        read_message::<Request>(&mut &u32::MAX.to_be_bytes()[..]).is_err(),
    );
    record(
        "unknown_version_rejected",
        validate(&Request {
            version: VERSION + 1,
            operation: Operation::Status {},
        })
        .is_err(),
    );
    record(
        "unknown_fields_rejected",
        serde_json::from_str::<Request>(
            r#"{"version":1,"operation":{"type":"status","extra":true}}"#,
        )
        .is_err(),
    );

    let mut history = History::new(4);
    history.append(b"abcdef");
    let chunk = history.read(0, 4).expect("history");
    record(
        "history_gap_and_offsets",
        chunk.gap && chunk.start == 2 && chunk.next == 6 && chunk.data == b"cdef",
    );
    record("future_cursor_rejected", history.read(7, 4).is_err());
    let mut controller = Controller::default();
    let old = controller.acquire(false).expect("first controller");
    record(
        "implicit_takeover_rejected",
        controller.acquire(false).is_err(),
    );
    let current = controller.acquire(true).expect("explicit takeover");
    record(
        "old_controller_fenced",
        controller.check(old).is_err() && controller.check(current).is_ok(),
    );
    record(
        "first_input_accepted",
        controller.prepare(current, 1, b"probe") == Ok(InputDecision::Write),
    );
    record(
        "uncertain_input_not_retried",
        controller.prepare(current, 1, b"probe").is_err(),
    );
    controller.written();
    record(
        "acknowledged_input_deduplicated",
        controller.prepare(current, 1, b"probe") == Ok(InputDecision::Duplicate),
    );
    record(
        "conflicting_input_rejected",
        controller.prepare(current, 1, b"changed").is_err(),
    );
    controller.release(current).expect("release");
    record(
        "released_controller_fenced",
        controller.check(current).is_err(),
    );
    println!(
        "{}",
        serde_json::json!({
            "schema": "dot-terminal.device-probe/1",
            "status": "pass",
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "checks": passed,
        })
    );
}
