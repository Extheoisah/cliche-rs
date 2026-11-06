//! Real HTTP/2 gRPC tests backed by a disposable mock cliche WebSocket.
mod common;
use cliche_client::Client;
use cliche_gateway::{
    Gateway, GatewayPolicy,
    proto::{
        self,
        cliche_service_client::ClicheServiceClient,
        cliche_service_server::{ClicheService, ClicheServiceServer},
    },
};
use common::*;
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::time::Duration;
use tokio::{sync::oneshot, task::JoinHandle};
use tokio_stream::wrappers::TcpListenerStream;
use tonic::{Code, Request, transport::Channel};

struct Harness {
    sdk: Client,
    service: Gateway,
    rpc: ClicheServiceClient<Channel>,
    address: String,
    stop: oneshot::Sender<()>,
    grpc: JoinHandle<()>,
    ws: JoinHandle<()>,
}
impl Harness {
    async fn start(policy: GatewayPolicy) -> Self {
        let (listener, endpoint) = listener().await;
        let ws = tokio::spawn(async move {
            let mut socket = accept(&listener).await;
            while let Some(Ok(frame)) = socket.next().await {
                let Ok(text) = frame.to_text() else {
                    continue;
                };
                let req: Value = serde_json::from_str(text).unwrap();
                match req["method"].as_str().unwrap() {
                    "never" => {}
                    "error" => {
                        send(
                            &mut socket,
                            json!({"id":req["id"],"error":{"code":1,"message":"upstream failure"}}),
                        )
                        .await
                    }
                    "trigger" => {
                        send(&mut socket, json!({"method":"payment_received","params":{"payment_hash":"hash","preimage":"secret","msatoshi":9007199254740993_i64}})).await;
                        respond(&mut socket, &req, json!(true)).await;
                    }
                    "flood" => {
                        for _ in 0..100 {
                            send(&mut socket, json!({"method":"future","params":{}})).await;
                        }
                        respond(&mut socket, &req, json!(true)).await;
                    }
                    _ => respond(&mut socket, &req, req["params"].clone()).await,
                }
            }
        });
        let sdk = Client::connect(endpoint).await.unwrap();
        let service = Gateway::new(sdk.clone(), policy).unwrap();
        let incoming = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = incoming.local_addr().unwrap().to_string();
        let (stop, stopped) = oneshot::channel();
        let rpc_service = service.clone();
        let grpc = tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(ClicheServiceServer::new(rpc_service))
                .serve_with_incoming_shutdown(TcpListenerStream::new(incoming), async {
                    let _ = stopped.await;
                })
                .await
                .unwrap();
        });
        let rpc = ClicheServiceClient::connect(format!("http://{address}"))
            .await
            .unwrap();
        Self {
            sdk,
            service,
            rpc,
            address,
            stop,
            grpc,
            ws,
        }
    }
    async fn finish(self) {
        self.sdk.close().await;
        let _ = self.stop.send(());
        self.grpc.await.unwrap();
        self.ws.await.unwrap();
    }
}
fn call(method: &str, params: Value) -> proto::CallRequest {
    proto::CallRequest {
        method: method.into(),
        params_json: params.to_string(),
        request_id: "same-label".into(),
    }
}
fn authenticated<T>(body: T) -> Request<T> {
    let mut request = Request::new(body);
    request
        .metadata_mut()
        .insert("authorization", "Bearer test-token".parse().unwrap());
    request
}
fn all_methods() -> GatewayPolicy {
    GatewayPolicy {
        allowed_methods: None,
        ..Default::default()
    }
}

#[tokio::test]
async fn grpc_calls_events_errors_and_large_integers() {
    let mut h = Harness::start(all_methods()).await;
    let mut events = h
        .rpc
        .subscribe_events(proto::SubscribeEventsRequest { methods: vec![] })
        .await
        .unwrap()
        .into_inner();
    let initial: Value =
        serde_json::from_str(&events.message().await.unwrap().unwrap().json).unwrap();
    assert_eq!(initial["state"], "connected");
    let mut clone = h.rpc.clone();
    let (first, second) = tokio::join!(
        h.rpc
            .call(call("echo", json!({"value":9007199254740993_i64}))),
        clone.call(call("echo", json!({"value":42})))
    );
    assert_eq!(
        serde_json::from_str::<Value>(&first.unwrap().into_inner().json).unwrap()["result"]["value"],
        9007199254740993_i64
    );
    assert_eq!(
        serde_json::from_str::<Value>(&second.unwrap().into_inner().json).unwrap()["result"]["value"],
        42
    );
    h.rpc.call(call("trigger", json!({}))).await.unwrap();
    let event: Value =
        serde_json::from_str(&events.message().await.unwrap().unwrap().json).unwrap();
    assert_eq!(event["method"], "payment_received");
    assert_eq!(event["params"]["msatoshi"], 9007199254740993_i64);
    let error: Value = serde_json::from_str(
        &h.rpc
            .call(call("error", json!({})))
            .await
            .unwrap()
            .into_inner()
            .json,
    )
    .unwrap();
    assert_eq!(error["error"]["upstream_code"], 1);
    drop(events);
    h.finish().await;
}
#[tokio::test]
async fn access_controls_apply_to_commands_events_and_status() {
    let mut h = Harness::start(GatewayPolicy {
        bearer_token: Some("test-token".into()),
        allowed_methods: Some(["ping".to_owned()].into_iter().collect()),
        allow_events: false,
        ..Default::default()
    })
    .await;
    assert_eq!(
        h.rpc
            .get_status(proto::GetStatusRequest {})
            .await
            .unwrap_err()
            .code(),
        Code::Unauthenticated
    );
    assert_eq!(
        h.rpc
            .call(authenticated(call("pay-invoice", json!({}))))
            .await
            .unwrap_err()
            .code(),
        Code::PermissionDenied
    );
    assert_eq!(
        h.rpc
            .subscribe_events(authenticated(proto::SubscribeEventsRequest {
                methods: vec![]
            }))
            .await
            .unwrap_err()
            .code(),
        Code::PermissionDenied
    );
    h.rpc
        .get_status(authenticated(proto::GetStatusRequest {}))
        .await
        .unwrap();
    assert_eq!(
        h.rpc
            .call(authenticated(proto::CallRequest {
                method: "ping".into(),
                params_json: "[]".into(),
                request_id: "".into()
            }))
            .await
            .unwrap_err()
            .code(),
        Code::InvalidArgument
    );
    h.finish().await;
}
#[tokio::test]
async fn gateway_timeout_preserves_execution_uncertainty() {
    let mut h = Harness::start(GatewayPolicy {
        request_timeout: Duration::from_millis(25),
        ..all_methods()
    })
    .await;
    let status = h.rpc.call(call("never", json!({}))).await.unwrap_err();
    assert_eq!(status.code(), Code::DeadlineExceeded);
    let detail: Value = serde_json::from_slice(status.details()).unwrap();
    assert_eq!(detail["error"]["execution"], "unknown");
    h.rpc.call(call("echo", json!({}))).await.unwrap();
    h.finish().await;
}
#[tokio::test]
async fn stalled_stream_is_terminated_and_releases_subscription_permit() {
    let mut h = Harness::start(GatewayPolicy {
        max_subscribers: 1,
        stream_capacity: 1,
        stream_stall_timeout: Duration::from_millis(25),
        ..all_methods()
    })
    .await;
    // Exercise the service stream without HTTP/2 prefetch to make backpressure deterministic.
    let mut stream = h
        .service
        .subscribe_events(Request::new(proto::SubscribeEventsRequest {
            methods: vec![],
        }))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        h.service
            .subscribe_events(Request::new(proto::SubscribeEventsRequest {
                methods: vec![]
            }))
            .await
            .err()
            .unwrap()
            .code(),
        Code::ResourceExhausted
    );
    h.rpc.call(call("flood", json!({}))).await.unwrap();
    tokio::time::sleep(Duration::from_millis(75)).await;
    assert_eq!(
        stream.next().await.unwrap().unwrap_err().code(),
        Code::ResourceExhausted
    );
    drop(stream);
    let next = h
        .service
        .subscribe_events(Request::new(proto::SubscribeEventsRequest {
            methods: vec![],
        }))
        .await
        .unwrap();
    drop(next);
    h.finish().await;
}
#[tokio::test]
#[ignore = "requires npm ci in examples/node; explicitly run for cross-language verification"]
async fn node_lossless_roundtrip() {
    let h = Harness::start(all_methods()).await;
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/node/test.mjs");
    let status = tokio::process::Command::new("node")
        .arg(path)
        .arg(&h.address)
        .status()
        .await
        .unwrap();
    assert!(status.success());
    h.finish().await;
}

#[tokio::test]
async fn default_policy_allows_invoice_commands_without_a_token() {
    let mut h = Harness::start(GatewayPolicy::default()).await;
    for method in ["create-invoice", "pay-invoice", "request-hc"] {
        // The mock echoes params. No live payment or channel command is sent.
        let response = h
            .rpc
            .call(call(method, json!({"mock":true})))
            .await
            .unwrap();
        let document: Value = serde_json::from_str(&response.into_inner().json).unwrap();
        assert_eq!(document["kind"], "response");
        assert_eq!(document["result"]["mock"], true);
    }
    h.rpc.get_status(proto::GetStatusRequest {}).await.unwrap();
    let events = h
        .rpc
        .subscribe_events(proto::SubscribeEventsRequest { methods: vec![] })
        .await
        .unwrap();
    drop(events);
    h.finish().await;
}
