//! Concurrent command and cancellation tests with a deterministic mock cliche.
mod common;
use cliche_client::{Client, ErrorKind, Execution};
use common::*;
use serde_json::{Value, json};
use std::time::Duration;

#[tokio::test]
async fn concurrent_out_of_order_foreign_and_null_replies() {
    let (listener, endpoint) = listener().await;
    let server = tokio::spawn(async move {
        let mut socket = accept(&listener).await;
        let mut requests = Vec::new();
        for _ in 0..100 {
            requests.push(request(&mut socket).await);
        }
        send(&mut socket, json!({"id":"foreign","result":"wrong"})).await;
        for req in requests.into_iter().rev() {
            respond(&mut socket, &req, req["params"]["value"].clone()).await;
        }
        let req = request(&mut socket).await;
        respond(&mut socket, &req, Value::Null).await;
        let _ = futures_util::StreamExt::next(&mut socket).await;
    });
    let client = Client::connect(endpoint).await.unwrap();
    let mut jobs = Vec::new();
    for value in 0..100 {
        let c = client.clone();
        jobs.push(tokio::spawn(async move {
            assert_eq!(c.call("echo", json!({"value":value})).await.unwrap(), value);
        }));
    }
    for job in jobs {
        job.await.unwrap();
    }
    assert_eq!(client.call("null", json!({})).await.unwrap(), Value::Null);
    client.close().await;
    server.await.unwrap();
}
#[tokio::test]
async fn timeouts_and_cancellation_release_capacity() {
    let (listener, endpoint) = listener().await;
    let (seen, mut seen_rx) = tokio::sync::mpsc::channel(8);
    let server = tokio::spawn(async move {
        let mut socket = accept(&listener).await;
        for _ in 0..2 {
            let req = request(&mut socket).await;
            seen.send(req).await.unwrap();
        }
        let req = request(&mut socket).await;
        respond(&mut socket, &req, json!({"ok":true})).await;
        let _ = futures_util::StreamExt::next(&mut socket).await;
    });
    let client = Client::builder(endpoint)
        .max_in_flight(1)
        .connect()
        .await
        .unwrap();
    let error = client
        .call_with_timeout("never", json!({}), Duration::from_millis(50))
        .await
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Timeout);
    assert_eq!(error.execution, Execution::Unknown);
    seen_rx.recv().await.unwrap();
    let c = client.clone();
    let cancelled = tokio::spawn(async move { c.call("cancel", json!({})).await });
    seen_rx.recv().await.unwrap();
    cancelled.abort();
    let _ = cancelled.await;
    assert_eq!(client.call("next", json!({})).await.unwrap()["ok"], true);
    client.close().await;
    server.await.unwrap();
}
#[tokio::test]
async fn disconnect_and_shared_close_resolve_waiters() {
    let (listener, endpoint) = listener().await;
    let server = tokio::spawn(async move {
        let mut socket = accept(&listener).await;
        request(&mut socket).await;
    });
    let client = Client::connect(endpoint).await.unwrap();
    let error = client.call("lost", json!({})).await.unwrap_err();
    assert_eq!(error.execution, Execution::Unknown);
    client.close().await;
    assert!(client.clone().call("no", json!({})).await.is_err());
    server.await.unwrap();
}
#[tokio::test]
async fn configuration_is_validated_without_network() {
    for endpoint in [
        "http://localhost/",
        "invalid",
        "ws://user:secret@localhost/",
    ] {
        assert!(Client::connect(endpoint).await.is_err());
    }
    assert!(
        Client::builder("ws://localhost")
            .max_in_flight(0)
            .connect()
            .await
            .is_err()
    );
}

#[tokio::test]
async fn server_heartbeat_and_last_handle_drop() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;
    let (listener, endpoint) = listener().await;
    let (pong, pong_rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let mut socket = accept(&listener).await;
        socket
            .send(Message::Ping(vec![1, 2, 3].into()))
            .await
            .unwrap();
        assert!(
            matches!(socket.next().await.unwrap().unwrap(), Message::Pong(payload) if payload.as_ref() == [1,2,3])
        );
        pong.send(()).unwrap();
        let _ = socket.next().await;
    });
    let client = Client::connect(endpoint).await.unwrap();
    tokio::time::timeout(Duration::from_secs(1), pong_rx)
        .await
        .unwrap()
        .unwrap();
    drop(client);
    tokio::time::timeout(Duration::from_secs(1), server)
        .await
        .unwrap()
        .unwrap();
}
#[tokio::test]
async fn identity_mismatch_and_message_limits_are_enforced() {
    let (listener, endpoint) = listener().await;
    let server = tokio::spawn(async move {
        let mut socket = accept(&listener).await;
        let req = request(&mut socket).await;
        assert_eq!(req["method"], "get-info");
        respond(&mut socket, &req, json!({"main_pubkey":"wrong"})).await;
    });
    assert_eq!(
        Client::builder(endpoint)
            .expected_pubkey("expected")
            .connect()
            .await
            .err()
            .unwrap()
            .kind,
        ErrorKind::Protocol
    );
    server.await.unwrap();
    let (listener, endpoint) = common::listener().await;
    let server = tokio::spawn(async move {
        let mut socket = accept(&listener).await;
        let req = request(&mut socket).await;
        respond(&mut socket, &req, json!({"huge":"x".repeat(2000)})).await;
    });
    let client = Client::builder(endpoint)
        .max_message_size(512)
        .connect()
        .await
        .unwrap();
    assert_eq!(
        client
            .call("x", json!({"huge":"x".repeat(2000)}))
            .await
            .unwrap_err()
            .execution,
        Execution::NotSent
    );
    assert!(client.call("small", json!({})).await.is_err());
    client.close().await;
    server.await.unwrap();
}

#[tokio::test]
async fn readiness_recovers_when_output_subscription_misses_first_ping() {
    let (listener, endpoint) = listener().await;
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        let first = request(&mut socket).await;
        assert_eq!(first["method"], "ping");
        // Simulate upstream publishing before this socket's topic subscription.
        let second = request(&mut socket).await;
        assert_eq!(second["method"], "ping");
        respond(&mut socket, &second, json!({"ping":"pong"})).await;
        let command = request(&mut socket).await;
        assert_eq!(command["method"], "once-only");
        respond(&mut socket, &command, json!(true)).await;
        let _ = futures_util::StreamExt::next(&mut socket).await;
    });
    let client = Client::builder(endpoint)
        .connect_timeout(Duration::from_secs(2))
        .connect()
        .await
        .unwrap();
    assert_eq!(client.call("once-only", json!({})).await.unwrap(), true);
    client.close().await;
    server.await.unwrap();
}
