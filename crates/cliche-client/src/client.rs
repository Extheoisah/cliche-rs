use crate::{
    Error, ErrorKind, Execution,
    events::{ConnectionState, EventRecord},
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    sync::{Semaphore, broadcast, mpsc, oneshot, watch},
    time::{Instant, timeout_at},
};

pub(crate) struct Pending {
    pub reply: oneshot::Sender<Result<Value, Error>>,
    pub sent: bool,
}
pub(crate) struct Active {
    pub writer: mpsc::Sender<Outbound>,
    pub session: String,
    pub counter: u64,
}
pub(crate) struct Shared {
    pub active: Option<Active>,
    pub pending: HashMap<String, Pending>,
}
pub(crate) struct State {
    pub shared: Mutex<Shared>,
    pub events: broadcast::Sender<EventRecord>,
    pub status: watch::Sender<ConnectionState>,
    pub stopping: AtomicBool,
}
impl State {
    pub fn lock(&self) -> std::sync::MutexGuard<'_, Shared> {
        // No user code or fallible work runs while holding this lock.
        self.shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
    pub fn set_status(&self, state: ConnectionState) {
        self.status.send_replace(state);
        let _ = self.events.send(EventRecord::Connection { state });
    }
    pub fn disconnect(&self, kind: ErrorKind) {
        let mut shared = self.lock();
        shared.active = None;
        for (_, pending) in shared.pending.drain() {
            let execution = if pending.sent {
                Execution::Unknown
            } else {
                Execution::NotSent
            };
            let _ = pending
                .reply
                .send(Err(Error::new(kind, "connection ended", execution)));
        }
    }
}
pub(crate) struct Outbound {
    pub id: String,
    pub text: String,
    pub deadline: Instant,
}
struct Handle {
    state: Arc<State>,
    stop: watch::Sender<bool>,
    finished: watch::Receiver<bool>,
    permits: Semaphore,
    config: ClientBuilder,
}
impl Drop for Handle {
    fn drop(&mut self) {
        self.state.stopping.store(true, Ordering::SeqCst);
        self.stop.send_replace(true);
    }
}

/// Cloneable asynchronous client. Clones share one connection and shutdown state.
#[derive(Clone)]
pub struct Client {
    inner: Arc<Handle>,
}

/// Connection configuration. Queues and frames are bounded by default.
#[derive(Clone)]
pub struct ClientBuilder {
    pub(crate) endpoint: String,
    #[cfg(feature = "tls")]
    pub(crate) tls_config: Option<Arc<rustls::ClientConfig>>,
    pub(crate) reconnect: Option<ReconnectPolicy>,
    pub(crate) expected_pubkey: Option<String>,
    pub(crate) expected_chain: Option<String>,
    pub(crate) connect_timeout: Duration,
    pub(crate) request_timeout: Duration,
    pub(crate) max_in_flight: usize,
    pub(crate) event_capacity: usize,
    pub(crate) max_message_size: usize,
}
/// Bounded opt-in reconnection after an established socket disconnects.
#[derive(Clone, Copy, Debug)]
pub struct ReconnectPolicy {
    /// Maximum attempts per disconnect. Each attempt also has a connect timeout.
    pub max_attempts: u32,
    /// Initial exponential backoff, with up to 25% additional jitter.
    pub initial_delay: Duration,
    /// Maximum delay before jitter.
    pub max_delay: Duration,
}
impl Default for ReconnectPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            initial_delay: Duration::from_millis(250),
            max_delay: Duration::from_secs(30),
        }
    }
}
impl ClientBuilder {
    /// Custom TLS trust/client-auth configuration for private wss deployments.
    /// The default uses WebPKI roots and an explicit ring provider, without global state.
    #[cfg(feature = "tls")]
    pub fn tls_config(mut self, config: Arc<rustls::ClientConfig>) -> Self {
        self.tls_config = Some(config);
        self
    }

    /// Retry lost connections without replaying any command. Disabled by default.
    pub fn reconnect(mut self, policy: ReconnectPolicy) -> Self {
        self.reconnect = Some(policy);
        self
    }
    /// Check the node pubkey through get-info on every connection.
    pub fn expected_pubkey(mut self, pubkey: impl Into<String>) -> Self {
        self.expected_pubkey = Some(pubkey.into());
        self
    }
    /// Check the chain hash through get-info on every connection.
    pub fn expected_chain(mut self, chain: impl Into<String>) -> Self {
        self.expected_chain = Some(chain.into());
        self
    }

    /// Deadline for connecting and verifying command readiness.
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }
    /// Default deadline including admission, transmission and response waiting.
    pub fn request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }
    /// Maximum concurrent calls and queued commands. Must be between 1 and 65536.
    pub fn max_in_flight(mut self, capacity: usize) -> Self {
        self.max_in_flight = capacity;
        self
    }
    /// Number of typed records retained for slow subscribers (Tokio rounds up).
    pub fn event_capacity(mut self, capacity: usize) -> Self {
        self.event_capacity = capacity;
        self
    }
    /// Maximum incoming/outgoing JSON message size in bytes.
    pub fn max_message_size(mut self, bytes: usize) -> Self {
        self.max_message_size = bytes;
        self
    }
    /// Connect and verify a correlated cliche `ping`. Internal readiness probes
    /// may repeat until the connect deadline; application commands never replay.
    /// Never waits for the one-shot `ready` event.
    pub async fn connect(self) -> Result<Client, Error> {
        self.validate()?;
        let socket = crate::connection::open(&self).await?;
        let (events, _) = broadcast::channel(self.event_capacity);
        let (status, _) = watch::channel(ConnectionState::Connected);
        let state = Arc::new(State {
            shared: Mutex::new(Shared {
                active: None,
                pending: HashMap::new(),
            }),
            events,
            status,
            stopping: AtomicBool::new(false),
        });
        let (stop, stop_rx) = watch::channel(false);
        let (done, finished) = watch::channel(false);
        let client = Client {
            inner: Arc::new(Handle {
                state: state.clone(),
                stop,
                finished,
                permits: Semaphore::new(self.max_in_flight),
                config: self.clone(),
            }),
        };
        // Install the writer before returning, so the first call can be admitted.
        let (tx, rx) = mpsc::channel(self.max_in_flight);
        state.lock().active = Some(Active {
            writer: tx,
            session: uuid::Uuid::new_v4().to_string(),
            counter: 0,
        });
        tokio::spawn(crate::connection::run(
            socket, rx, state, stop_rx, done, self,
        ));
        Ok(client)
    }
    fn validate(&self) -> Result<(), Error> {
        if let Some(policy) = self.reconnect
            && (policy.max_attempts == 0
                || policy.max_attempts > 100
                || policy.initial_delay.is_zero()
                || policy.initial_delay > policy.max_delay
                || policy.max_delay > Duration::from_secs(300))
        {
            return Err(Error::invalid("invalid reconnect policy"));
        }
        let uri: tokio_tungstenite::tungstenite::http::Uri = self
            .endpoint
            .parse()
            .map_err(|_| Error::invalid("invalid websocket URL"))?;
        if !matches!(uri.scheme_str(), Some("ws" | "wss"))
            || uri.host().is_none()
            || uri.authority().is_some_and(|a| a.as_str().contains('@'))
        {
            return Err(Error::invalid(
                "expected ws:// or wss:// URL without credentials",
            ));
        }
        if uri.scheme_str() == Some("wss") && !cfg!(feature = "tls") {
            return Err(Error::invalid("wss requires the tls feature"));
        }
        if self.connect_timeout.is_zero()
            || self.request_timeout.is_zero()
            || self.connect_timeout > Duration::from_secs(86400)
            || self.request_timeout > Duration::from_secs(86400)
        {
            return Err(Error::invalid("timeouts must be between zero and one day"));
        }
        if !(1..=65536).contains(&self.max_in_flight)
            || !(1..=65536).contains(&self.event_capacity)
            || !(1..=64 * 1024 * 1024).contains(&self.max_message_size)
        {
            return Err(Error::invalid(
                "capacity or message size outside supported limits",
            ));
        }
        Ok(())
    }
}
impl Client {
    /// Configure a client for a WebSocket URL.
    pub fn builder(endpoint: impl Into<String>) -> ClientBuilder {
        ClientBuilder {
            endpoint: endpoint.into(),
            #[cfg(feature = "tls")]
            tls_config: None,
            reconnect: None,
            expected_pubkey: None,
            expected_chain: None,
            connect_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(30),
            max_in_flight: 128,
            event_capacity: 256,
            max_message_size: 4 * 1024 * 1024,
        }
    }
    /// Connect using default limits.
    pub async fn connect(endpoint: impl Into<String>) -> Result<Self, Error> {
        Self::builder(endpoint).connect().await
    }
    /// Snapshot connection status. Connection-ready does not imply payment-ready.
    pub fn status(&self) -> ConnectionState {
        *self.inner.state.status.borrow()
    }
    /// Watch the latest status without polling. Changes may be coalesced.
    pub fn watch_status(&self) -> watch::Receiver<ConnectionState> {
        self.inner.state.status.subscribe()
    }
    /// Subscribe to typed notifications and local status/gap/decode records.
    pub fn subscribe(&self) -> crate::events::Subscription {
        let receiver = self.inner.state.events.subscribe();
        crate::events::Subscription {
            receiver,
            initial: Some(self.status()),
            status: self.watch_status(),
            stream_id: uuid::Uuid::new_v4().to_string(),
            sequence: 0,
            ended: false,
            methods: Vec::new(),
        }
    }
    /// Send a method with object params, returning its JSON result. Never replayed.
    pub async fn call(&self, method: &str, params: Value) -> Result<Value, Error> {
        self.call_with_timeout(method, params, self.inner.config.request_timeout)
            .await
    }
    /// Per-call deadline, including admission. Cancellation only stops waiting.
    pub async fn call_with_timeout(
        &self,
        method: &str,
        params: Value,
        duration: Duration,
    ) -> Result<Value, Error> {
        if method.is_empty()
            || method.len() > 128
            || !method
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            || !params.is_object()
        {
            return Err(Error::invalid(
                "method must be a name and params must be an object",
            ));
        }
        crate::commands::validate_numeric_params(method, &params)?;
        if duration.is_zero() || duration > Duration::from_secs(86400) {
            return Err(Error::invalid(
                "call timeout must be between zero and one day",
            ));
        }
        if self.status() != ConnectionState::Connected
            || self.inner.state.stopping.load(Ordering::SeqCst)
        {
            return Err(Error::new(
                ErrorKind::Connection,
                "client is not connected",
                Execution::NotSent,
            ));
        }
        let deadline = Instant::now() + duration;
        let _permit = timeout_at(deadline, self.inner.permits.acquire())
            .await
            .map_err(|_| {
                Error::new(
                    ErrorKind::Timeout,
                    "admission deadline exceeded",
                    Execution::NotSent,
                )
            })?
            .map_err(|_| Error::new(ErrorKind::Closed, "client closed", Execution::NotSent))?;
        let (reply, receiver) = oneshot::channel();
        let (id, writer) = {
            let mut shared = self.inner.state.lock();
            if self.inner.state.stopping.load(Ordering::SeqCst) {
                return Err(Error::new(
                    ErrorKind::Closed,
                    "client closed",
                    Execution::NotSent,
                ));
            }
            let active = shared.active.as_mut().ok_or_else(|| {
                Error::new(
                    ErrorKind::Connection,
                    "connection unavailable",
                    Execution::NotSent,
                )
            })?;
            active.counter = active.counter.checked_add(1).ok_or_else(|| {
                Error::new(
                    ErrorKind::Closed,
                    "request IDs exhausted",
                    Execution::NotSent,
                )
            })?;
            let id = format!("{}:{}", active.session, active.counter);
            let writer = active.writer.clone();
            shared
                .pending
                .insert(id.clone(), Pending { reply, sent: false });
            (id, writer)
        };
        let mut guard = PendingGuard {
            state: &self.inner.state,
            id: &id,
        };
        let text = json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}).to_string();
        if text.len() > self.inner.config.max_message_size {
            return Err(Error::invalid("outgoing message exceeds configured limit"));
        }
        let outgoing = Outbound {
            id: id.clone(),
            text,
            deadline,
        };
        timeout_at(deadline, writer.send(outgoing))
            .await
            .map_err(|_| guard.timeout())?
            .map_err(|_| {
                Error::new(
                    ErrorKind::Connection,
                    "connection ended before transmission",
                    Execution::NotSent,
                )
            })?;
        timeout_at(deadline, receiver)
            .await
            .map_err(|_| guard.timeout())?
            .map_err(|_| {
                Error::new(
                    ErrorKind::Closed,
                    "connection task stopped",
                    Execution::Unknown,
                )
            })?
    }
    /// Close the connection for every clone and wait for socket tasks to stop.
    pub async fn close(&self) {
        self.begin_close();
        let mut finished = self.inner.finished.clone();
        let _ = finished.wait_for(|done| *done).await;
    }
    pub(crate) fn begin_close(&self) {
        self.inner.state.stopping.store(true, Ordering::SeqCst);
        self.inner.permits.close();
        self.inner.stop.send_replace(true);
    }
}
struct PendingGuard<'a> {
    state: &'a State,
    id: &'a str,
}
impl PendingGuard<'_> {
    fn timeout(&mut self) -> Error {
        let execution = self
            .state
            .lock()
            .pending
            .get(self.id)
            .map_or(Execution::Unknown, |p| {
                if p.sent {
                    Execution::Unknown
                } else {
                    Execution::NotSent
                }
            });
        Error::new(ErrorKind::Timeout, "response deadline exceeded", execution)
    }
}
impl Drop for PendingGuard<'_> {
    fn drop(&mut self) {
        self.state.lock().pending.remove(self.id);
    }
}
