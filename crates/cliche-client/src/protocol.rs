use crate::{
    Error, ErrorKind, Execution,
    events::{ClicheEvent, DecodeReason, EventDecodeError, EventRecord},
};
use serde_json::Value;

pub(crate) enum Incoming {
    Reply(String, Result<Value, Error>),
    Event(EventRecord),
    Invalid,
}
pub(crate) fn parse(text: &str) -> Incoming {
    let Ok(Value::Object(mut object)) = serde_json::from_str(text) else {
        return Incoming::Invalid;
    };
    if object.contains_key("id") {
        if object.contains_key("method") {
            return Incoming::Invalid;
        }
        let Some(Value::String(id)) = object.remove("id") else {
            return Incoming::Invalid;
        };
        let result = match (object.remove("result"), object.remove("error")) {
            (Some(result), None) => Ok(result),
            (None, Some(error)) => {
                let (Some(code), Some(message)) = (
                    error.get("code").and_then(Value::as_i64),
                    error.get("message").and_then(Value::as_str),
                ) else {
                    return Incoming::Invalid;
                };
                Err(Error {
                    kind: ErrorKind::Upstream,
                    message: message.into(),
                    execution: Execution::Unknown,
                    upstream_code: Some(code),
                })
            }
            _ => return Incoming::Invalid,
        };
        Incoming::Reply(id, result)
    } else if let Some(Value::String(method)) = object.remove("method") {
        let event = match object.remove("params") {
            Some(params) => ClicheEvent::decode(&method, params),
            None => Err(EventDecodeError {
                method,
                reason: DecodeReason::InvalidPayload,
            }),
        };
        Incoming::Event(match event {
            Ok(event) => EventRecord::Notification { event },
            Err(error) => EventRecord::InvalidNotification { error },
        })
    } else {
        Incoming::Invalid
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn response_shapes_and_null_results() {
        assert!(
            matches!(parse(r#"{"id":"a","result":null}"#), Incoming::Reply(id, Ok(Value::Null)) if id == "a")
        );
        assert!(matches!(
            parse(r#"{"id":"a","error":{"code":1,"message":"bad"}}"#),
            Incoming::Reply(
                _,
                Err(Error {
                    upstream_code: Some(1),
                    ..
                })
            )
        ));
        for text in [
            r#"{"id":3,"result":0}"#,
            r#"{"id":"a","result":0,"error":{}}"#,
            r#"{"id":"a","method":"ping","result":0}"#,
            "[]",
            "invalid",
        ] {
            assert!(matches!(parse(text), Incoming::Invalid));
        }
    }
    #[test]
    fn malformed_known_notification_is_observable() {
        assert!(matches!(
            parse(r#"{"method":"payment_received","params":{}}"#),
            Incoming::Event(EventRecord::InvalidNotification { .. })
        ));
    }
}
