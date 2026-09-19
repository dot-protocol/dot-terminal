//! JNI boundary validates every request and response using the shared Rust protocol.
use dot_terminal_protocol::{MAX_FRAME, Request, Response, validate};
pub fn checked_json(input: &str, request: bool) -> Result<String, String> {
    if input.len() > MAX_FRAME {
        return Err("frame limit exceeded".into());
    }
    if request {
        let value: Request = serde_json::from_str(input).map_err(|e| e.to_string())?;
        validate(&value).map_err(str::to_owned)?;
        serde_json::to_string(&value).map_err(|e| e.to_string())
    } else {
        let value: Response = serde_json::from_str(input).map_err(|e| e.to_string())?;
        serde_json::to_string(&value).map_err(|e| e.to_string())
    }
}
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_world_dot_terminal_NativeBridge_check(
    mut env: jni::JNIEnv,
    _class: jni::objects::JClass,
    input: jni::objects::JString,
    request: jni::sys::jboolean,
) -> jni::sys::jstring {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let text: String = env.get_string(&input).map_err(|e| e.to_string())?.into();
        checked_json(&text, request != 0)
    }));
    match result {
        Ok(Ok(s)) => match env.new_string(s) {
            Ok(s) => s.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        failure => {
            let message = match failure {
                Ok(Err(e)) => e,
                _ => "native protocol failure".into(),
            };
            let _ = env.throw_new("java/lang/IllegalArgumentException", message);
            std::ptr::null_mut()
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_untrusted_messages_at_boundary() {
        assert!(checked_json(r#"{"version":99,"operation":{"type":"status"}}"#, true).is_err());
        assert!(checked_json(r#"{"type":"unknown"}"#, false).is_err());
        assert!(checked_json(&"x".repeat(MAX_FRAME + 1), false).is_err());
    }
}
