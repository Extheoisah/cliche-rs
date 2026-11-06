//! Exact helper wire parameters and validation.
mod common;
use cliche_client::{Client, CreateInvoiceParams, ErrorKind, PayInvoiceParams};
use common::*;
use serde_json::json;
#[tokio::test]
async fn helper_params_and_validation() {
    let (listener, endpoint) = listener().await;
    let server = tokio::spawn(async move {
        let mut socket = accept(&listener).await;
        for (method, params) in [
            (
                "create-invoice",
                json!({"msatoshi":9007199254740993_i64,"description_hash":"a".repeat(64),"label":"test"}),
            ),
            ("pay-invoice", json!({"invoice":"lnbc-example"})),
            ("check-payment", json!({"hash":"b".repeat(64)})),
            ("list-payments", json!({})),
            ("get-address", json!({})),
        ] {
            let req = request(&mut socket).await;
            assert_eq!(req["method"], method);
            assert_eq!(req["params"], params);
            respond(&mut socket, &req, json!({})).await;
        }
        let _ = futures_util::StreamExt::next(&mut socket).await;
    });
    let client = Client::connect(endpoint).await.unwrap();
    let error = client
        .create_invoice(CreateInvoiceParams {
            msatoshi: Some(-1),
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::InvalidArgument);
    assert!(client.check_payment("invalid").await.is_err());
    client
        .create_invoice(CreateInvoiceParams {
            msatoshi: Some(9007199254740993),
            description_hash: Some("a".repeat(64)),
            label: Some("test".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    client
        .pay_invoice(PayInvoiceParams {
            invoice: "lnbc-example".into(),
            msatoshi: None,
        })
        .await
        .unwrap();
    client.check_payment(&"b".repeat(64)).await.unwrap();
    client.list_payments(None).await.unwrap();
    client.get_address().await.unwrap();
    client.close().await;
    server.await.unwrap();
}
