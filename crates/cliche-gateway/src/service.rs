use crate::proto::{self, cliche_service_server::ClicheService};
use cliche_client::{Client, Error, ErrorKind, events::ConnectionState};
use futures_util::Stream;
use serde_json::{Value, json};
use std::{collections::HashSet, pin::Pin, sync::Arc, time::Duration};
use subtle::ConstantTimeEq;
use tokio::sync::{Semaphore, mpsc, watch};
use tonic::{Request, Response, Status};

/// Admission, access and streaming limits. One policy represents one wallet trust boundary.
#[derive(Clone)]
pub struct GatewayPolicy {
    /// Optional bearer token. Do not expose plaintext gRPC outside a trusted network.
    pub bearer_token: Option<String>,
    /// Allowed command methods; None allows every method. Events are separately gated.
    pub allowed_methods: Option<HashSet<String>>,
    /// Permit wallet-wide event subscriptions.
    pub allow_events: bool,
    /// Global number of active RPC calls (including status requests).
    pub max_calls: usize,
    /// Global number of active subscriptions.
    pub max_subscribers: usize,
    /// Per-subscriber output queue.
    pub stream_capacity: usize,
    /// Maximum time a subscriber may block its output queue.
    pub stream_stall_timeout: Duration,
    /// Maximum command execution deadline, including client admission.
    pub request_timeout: Duration,
    /// Maximum serialized params/document bytes. Configure tonic message limits too.
    pub max_json_bytes: usize,
}
impl Default for GatewayPolicy {
    fn default() -> Self {
        Self {
            bearer_token: None,
            allowed_methods: Some(
                [
                    "ping",
                    "get-info",
                    "check-payment",
                    "list-payments",
                    "get-address",
                ]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            ),
            allow_events: true,
            max_calls: 128,
            max_subscribers: 64,
            stream_capacity: 64,
            stream_stall_timeout: Duration::from_secs(30),
            request_timeout: Duration::from_secs(30),
            max_json_bytes: 4 * 1024 * 1024,
        }
    }
}
/// Cloneable gRPC service. All callers share one correlated cliche connection.
#[derive(Clone)]
pub struct Gateway {
    client: Client,
    policy: Arc<GatewayPolicy>,
    calls: Arc<Semaphore>,
    subscribers: Arc<Semaphore>,
}
impl Gateway {
    /// Validate policy and construct a service. Does not start a listener.
    pub fn new(client: Client, policy: GatewayPolicy) -> Result<Self, Status> {
        if policy.max_calls == 0
            || policy.max_calls > 65536
            || policy.max_subscribers == 0
            || policy.max_subscribers > 65536
            || policy.stream_capacity == 0
            || policy.stream_capacity > 65536
            || policy.max_json_bytes == 0
            || policy.max_json_bytes > 64 * 1024 * 1024
            || policy.request_timeout.is_zero()
            || policy.request_timeout > Duration::from_secs(86400)
            || policy.stream_stall_timeout.is_zero()
            || policy.stream_stall_timeout > Duration::from_secs(86400)
            || policy
                .bearer_token
                .as_ref()
                .is_some_and(|s| s.is_empty() || !s.is_ascii())
        {
            return Err(Status::invalid_argument("invalid gateway policy"));
        }
        Ok(Self {
            client,
            calls: Arc::new(Semaphore::new(policy.max_calls)),
            subscribers: Arc::new(Semaphore::new(policy.max_subscribers)),
            policy: Arc::new(policy),
        })
    }
    fn authorize<T>(&self, request: &Request<T>) -> Result<(), Status> {
        if let Some(token) = &self.policy.bearer_token {
            let provided = request
                .metadata()
                .get("authorization")
                .and_then(|s| s.to_str().ok())
                .and_then(|s| s.strip_prefix("Bearer "))
                .unwrap_or("");
            if !bool::from(provided.as_bytes().ct_eq(token.as_bytes())) {
                return Err(Status::unauthenticated("valid bearer token required"));
            }
        }
        Ok(())
    }
}
fn error_document(error: &Error, request_id: &str) -> Value {
    json!({"schema_version":1,"kind":"error","request_id":request_id,"error":error})
}
fn error_status(error: Error, request_id: &str) -> Status {
    let code = match error.kind {
        ErrorKind::InvalidArgument => tonic::Code::InvalidArgument,
        ErrorKind::Timeout => tonic::Code::DeadlineExceeded,
        ErrorKind::Overloaded => tonic::Code::ResourceExhausted,
        ErrorKind::Protocol => tonic::Code::Internal,
        _ => tonic::Code::Unavailable,
    };
    Status::with_details(
        code,
        "cliche command failed; inspect JSON status details",
        error_document(&error, request_id)
            .to_string()
            .into_bytes()
            .into(),
    )
}
fn timeout_header<T>(request: &Request<T>, maximum: Duration) -> Result<Duration, Status> {
    let Some(raw) = request.metadata().get("grpc-timeout") else {
        return Ok(maximum);
    };
    let raw = raw
        .to_str()
        .map_err(|_| Status::invalid_argument("invalid grpc-timeout"))?;
    if raw.len() < 2 || raw.len() > 9 {
        return Err(Status::invalid_argument("invalid grpc-timeout"));
    }
    let (digits, unit) = raw.split_at(raw.len() - 1);
    if !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(Status::invalid_argument("invalid grpc-timeout"));
    }
    let n: u64 = digits
        .parse()
        .map_err(|_| Status::invalid_argument("invalid grpc-timeout"))?;
    let duration = match unit {
        "H" => Duration::from_secs(n * 3600),
        "M" => Duration::from_secs(n * 60),
        "S" => Duration::from_secs(n),
        "m" => Duration::from_millis(n),
        "u" => Duration::from_micros(n),
        "n" => Duration::from_nanos(n),
        _ => return Err(Status::invalid_argument("invalid grpc-timeout")),
    };
    if duration.is_zero() {
        return Err(Status::deadline_exceeded("deadline already expired"));
    }
    Ok(duration.min(maximum))
}
#[tonic::async_trait]
impl ClicheService for Gateway {
    async fn call(
        &self,
        request: Request<proto::CallRequest>,
    ) -> Result<Response<proto::JsonDocument>, Status> {
        self.authorize(&request)?;
        let timeout = timeout_header(&request, self.policy.request_timeout)?;
        let _permit = self
            .calls
            .try_acquire()
            .map_err(|_| Status::resource_exhausted("too many active calls"))?;
        let request = request.into_inner();
        if request.request_id.len() > 256
            || request.method.len() > 128
            || request.params_json.len() > self.policy.max_json_bytes
        {
            return Err(Status::invalid_argument(
                "request exceeds configured limits",
            ));
        }
        if self
            .policy
            .allowed_methods
            .as_ref()
            .is_some_and(|methods| !methods.contains(&request.method))
        {
            return Err(Status::permission_denied("method is not allowed"));
        }
        let params: Value = serde_json::from_str(&request.params_json)
            .map_err(|_| Status::invalid_argument("params_json must be valid JSON"))?;
        if !params.is_object() {
            return Err(Status::invalid_argument(
                "params_json must be a JSON object",
            ));
        }
        let document = match self.client.call_with_timeout(&request.method, params, timeout).await {
            Ok(result) => json!({"schema_version":1,"kind":"response","request_id":request.request_id,"result":result}),
            Err(error) if error.kind == ErrorKind::Upstream => error_document(&error, &request.request_id),
            Err(error) => return Err(error_status(error, &request.request_id)),
        }.to_string();
        if document.len() > self.policy.max_json_bytes {
            return Err(Status::resource_exhausted(
                "response document exceeds configured limit",
            ));
        }
        Ok(Response::new(proto::JsonDocument { json: document }))
    }
    type SubscribeEventsStream =
        Pin<Box<dyn Stream<Item = Result<proto::JsonDocument, Status>> + Send>>;
    async fn subscribe_events(
        &self,
        request: Request<proto::SubscribeEventsRequest>,
    ) -> Result<Response<Self::SubscribeEventsStream>, Status> {
        self.authorize(&request)?;
        if !self.policy.allow_events {
            return Err(Status::permission_denied("event subscriptions disabled"));
        }
        let methods = request.into_inner().methods;
        if methods.len() > 128 || methods.iter().any(|m| m.is_empty() || m.len() > 128) {
            return Err(Status::invalid_argument("invalid event method filters"));
        }
        let permit = self
            .subscribers
            .clone()
            .try_acquire_owned()
            .map_err(|_| Status::resource_exhausted("too many subscriptions"))?;
        let mut events = self.client.subscribe().with_methods(methods);
        let (tx, mut rx) = mpsc::channel(self.policy.stream_capacity);
        let (terminal, mut termination) = watch::channel(None::<Status>);
        let timeout = self.policy.stream_stall_timeout;
        let max_size = self.policy.max_json_bytes;
        tokio::spawn(async move {
            let _permit = permit;
            loop {
                let delivery = tokio::select! {
                    _ = tx.closed() => break,
                    delivery = events.recv() => match delivery { Some(delivery) => delivery, None => break },
                };
                let Ok(document) = serde_json::to_string(&delivery) else {
                    terminal.send_replace(Some(Status::internal("event serialization failed")));
                    break;
                };
                if document.len() > max_size {
                    terminal.send_replace(Some(Status::resource_exhausted(
                        "event document exceeds configured limit",
                    )));
                    break;
                }
                match tokio::time::timeout(timeout, tx.send(proto::JsonDocument { json: document }))
                    .await
                {
                    Ok(Ok(())) => {}
                    Ok(Err(_)) => break,
                    Err(_) => {
                        terminal.send_replace(Some(Status::resource_exhausted(
                            "event subscriber stalled; reconcile before resubscribing",
                        )));
                        break;
                    }
                }
            }
        });
        let stream = async_stream::stream! {
            let mut terminal_open = true;
            loop {
                let next = tokio::select! {
                    biased;
                    changed = termination.changed(), if terminal_open => {
                        if changed.is_err() { terminal_open = false; }
                        termination.borrow().clone().map(Err)
                    }
                    document = rx.recv() => document.map(Ok),
                };
                match next {
                    Some(Ok(document)) => yield Ok(document),
                    Some(Err(error)) => { yield Err(error); break; }
                    None if rx.is_closed() && rx.is_empty() => break,
                    None => {},
                }
            }
        };
        Ok(Response::new(Box::pin(stream)))
    }
    async fn get_status(
        &self,
        request: Request<proto::GetStatusRequest>,
    ) -> Result<Response<proto::JsonDocument>, Status> {
        self.authorize(&request)?;
        let _permit = self
            .calls
            .try_acquire()
            .map_err(|_| Status::resource_exhausted("too many active calls"))?;
        let state = self.client.status();
        Ok(Response::new(proto::JsonDocument { json: json!({"schema_version":1,"kind":"status","state":state,"connected":state == ConnectionState::Connected,"typed_events":true,"event_replay":false}).to_string() }))
    }
}
