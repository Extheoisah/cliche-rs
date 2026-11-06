//! Event contract tests against the pinned source shapes.
use cliche_client::events::*;
use serde_json::{Value, json};

#[test]
fn known_events_are_typed_and_preserve_extensions() {
    let wire = json!({"method":"payment_received","params":{"payment_hash":"hash","preimage":"secret","msatoshi":9007199254740993_i64,"future":true}});
    let event: ClicheEvent = serde_json::from_value(wire.clone()).unwrap();
    let ClicheEvent::PaymentReceived(payment) = &event else {
        panic!("wrong variant")
    };
    assert_eq!(payment.msatoshi, Some(9007199254740993));
    assert_eq!(payment.extra["future"], true);
    assert_eq!(serde_json::to_value(&event).unwrap(), wire);
    assert!(!format!("{event:?}").contains("secret"));
}
#[test]
fn optional_fields_are_not_fabricated() {
    let received = ClicheEvent::decode(
        "payment_received",
        json!({"payment_hash":"h","preimage":"p"}),
    )
    .unwrap();
    let ClicheEvent::PaymentReceived(payment) = &received else {
        panic!()
    };
    assert_eq!(payment.msatoshi, None);
    assert!(
        serde_json::to_value(received).unwrap()["params"]
            .get("msatoshi")
            .is_none()
    );
    let succeeded = ClicheEvent::decode(
        "payment_succeeded",
        json!({"payment_hash":"h","preimage":"p","msatoshi":1,"fee_msatoshi":0}),
    )
    .unwrap();
    assert!(matches!(
        succeeded,
        ClicheEvent::PaymentSucceeded(PaymentSucceededEvent { parts: None, .. })
    ));
}
#[test]
fn malformed_known_is_not_unknown_and_diagnostics_are_safe() {
    let error = ClicheEvent::decode(
        "payment_succeeded",
        json!({"preimage":"secret","msatoshi":"secret"}),
    )
    .unwrap_err();
    assert!(!format!("{error:?}").contains("secret"));
    assert_eq!(error.method, "payment_succeeded");
    let unknown = ClicheEvent::decode("future", json!([1, "x"])).unwrap();
    assert!(matches!(unknown, ClicheEvent::Unknown(_)));
    assert_eq!(
        serde_json::to_value(unknown).unwrap()["params"],
        json!([1, "x"])
    );
}
#[test]
fn remaining_methods_decode_and_roundtrip() {
    for (method, params) in [
        ("ready", json!({})),
        (
            "payment_failed",
            json!({"payment_hash":"h","parts":2,"failure":["failed"],"routes":["route"]}),
        ),
        (
            "lnurlpay_params",
            json!({"min":1,"max":9,"callback":"https://example.org","description":"x","comment":0,"payerdata":{"auth":true,"name":false,"pubkey":false,"extension":1}}),
        ),
        ("lnurlpay_success", json!({"success_action":"done"})),
    ] {
        let event = ClicheEvent::decode(method, params.clone()).unwrap();
        let record = EventRecord::Notification { event };
        let wire = serde_json::to_value(record).unwrap();
        assert_eq!(wire["method"], method);
        assert_eq!(wire["params"], params);
        let _: EventRecord = serde_json::from_value::<Value>(wire.clone())
            .and_then(serde_json::from_value)
            .unwrap();
    }
}
