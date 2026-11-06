//! Typed delivery, backpressure and reconnect contracts.
mod common;
use cliche_client::{Client, ReconnectPolicy, events::*};
use common::*;
use serde_json::json;
use std::time::Duration;

#[tokio::test]
async fn slow_filtered_subscriber_gets_gap_without_blocking_calls() {
    let (listener, endpoint) = listener().await;
    let server = tokio::spawn(async move {
        let mut socket = accept(&listener).await;
        let req = request(&mut socket).await;
        for i in 0..40 {
            send(&mut socket, json!({"method":"payment_received","params":{"payment_hash":i.to_string(),"preimage":"p"}})).await;
        }
        send(
            &mut socket,
            json!({"method":"payment_received","params":{"msatoshi":"secret"}}),
        )
        .await;
        respond(&mut socket, &req, json!(true)).await;
        let _ = futures_util::StreamExt::next(&mut socket).await;
    });
    let client = Client::builder(endpoint)
        .event_capacity(4)
        .connect()
        .await
        .unwrap();
    let mut events = client.subscribe().with_methods(["other".into()]);
    assert!(matches!(
        events.recv().await.unwrap().record,
        EventRecord::Connection {
            state: ConnectionState::Connected
        }
    ));
    assert_eq!(client.call("trigger", json!({})).await.unwrap(), true);
    assert!(matches!(
        events.recv().await.unwrap().record,
        EventRecord::Gap {
            gap: EventGap {
                reason: GapReason::SubscriberLagged,
                missed_count: Some(_)
            }
        }
    ));
    assert!(matches!(
        events.recv().await.unwrap().record,
        EventRecord::InvalidNotification { .. }
    ));
    client.close().await;
    assert!(matches!(
        events.recv().await.unwrap().record,
        EventRecord::Connection {
            state: ConnectionState::Closed
        }
    ));
    assert!(events.recv().await.is_none());
    server.await.unwrap();
}
#[tokio::test]
async fn reconnect_uses_new_ids_and_never_replays_commands() {
    let (listener, endpoint) = listener().await;
    let server = tokio::spawn(async move {
        let mut first = accept(&listener).await;
        let lost = request(&mut first).await;
        drop(first);
        let mut next = accept(&listener).await;
        let req = request(&mut next).await;
        assert_eq!(req["method"], "new-command");
        assert_ne!(req["id"], lost["id"]);
        respond(&mut next, &req, json!(true)).await;
        let _ = futures_util::StreamExt::next(&mut next).await;
    });
    let client = Client::builder(endpoint)
        .reconnect(ReconnectPolicy {
            max_attempts: 2,
            initial_delay: Duration::from_millis(20),
            max_delay: Duration::from_millis(20),
        })
        .connect()
        .await
        .unwrap();
    let mut events = client.subscribe();
    assert!(client.call("do-not-replay", json!({})).await.is_err());
    let mut status = client.watch_status();
    tokio::time::timeout(
        Duration::from_secs(2),
        status.wait_for(|s| *s == ConnectionState::Connected),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(client.call("new-command", json!({})).await.unwrap(), true);
    let mut gap_seen = false;
    while let Some(record) = tokio::time::timeout(Duration::from_millis(20), events.recv())
        .await
        .ok()
        .flatten()
    {
        gap_seen |= matches!(record.record, EventRecord::Gap { .. });
    }
    assert!(gap_seen);
    client.close().await;
    server.await.unwrap();
}
