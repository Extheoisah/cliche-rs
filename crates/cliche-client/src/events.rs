//! Typed cliche notifications and observable delivery failures.
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Unknown fields retained for forward compatibility.
pub type ExtensionFields = Map<String, Value>;

/// Payload for `ReadyEvent`. Debug output redacts all payload fields.
#[derive(Clone, Serialize, Deserialize)]
pub struct ReadyEvent {
    /// Additional upstream fields.
    #[serde(flatten)]
    pub extra: ExtensionFields,
}
impl std::fmt::Debug for ReadyEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ReadyEvent { [redacted] }")
    }
}

/// Payload for `PaymentReceivedEvent`. Debug output redacts all payload fields.
#[derive(Clone, Serialize, Deserialize)]
pub struct PaymentReceivedEvent {
    /// Payment hash in hexadecimal.
    pub payment_hash: String,
    /// Secret payment preimage in hexadecimal.
    pub preimage: String,
    /// Received amount in millisatoshis, when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub msatoshi: Option<i64>,
    /// Additional upstream fields.
    #[serde(flatten)]
    pub extra: ExtensionFields,
}
impl std::fmt::Debug for PaymentReceivedEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PaymentReceivedEvent { [redacted] }")
    }
}

/// Payload for `PaymentSucceededEvent`. Debug output redacts all payload fields.
#[derive(Clone, Serialize, Deserialize)]
pub struct PaymentSucceededEvent {
    /// Payment hash in hexadecimal.
    pub payment_hash: String,
    /// Secret payment preimage in hexadecimal.
    pub preimage: String,
    /// Amount in millisatoshis.
    pub msatoshi: i64,
    /// Fee in millisatoshis.
    pub fee_msatoshi: i64,
    /// Shard count, absent in the pinned cliche revision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parts: Option<u64>,
    /// Additional upstream fields.
    #[serde(flatten)]
    pub extra: ExtensionFields,
}
impl std::fmt::Debug for PaymentSucceededEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PaymentSucceededEvent { [redacted] }")
    }
}

/// Payload for `PaymentFailedEvent`. Debug output redacts all payload fields.
#[derive(Clone, Serialize, Deserialize)]
pub struct PaymentFailedEvent {
    /// Payment hash in hexadecimal.
    pub payment_hash: String,
    /// Shard count.
    pub parts: u64,
    /// Upstream failure descriptions; may contain sensitive information.
    pub failure: Vec<String>,
    /// Attempted routes, when supplied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routes: Option<Vec<String>>,
    /// Additional upstream fields.
    #[serde(flatten)]
    pub extra: ExtensionFields,
}
impl std::fmt::Debug for PaymentFailedEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PaymentFailedEvent { [redacted] }")
    }
}

/// Payload for `LnurlPayerData`. Debug output redacts all payload fields.
#[derive(Clone, Serialize, Deserialize)]
pub struct LnurlPayerData {
    /// Authentication data supported.
    pub auth: bool,
    /// Name supported.
    pub name: bool,
    /// Public key supported.
    pub pubkey: bool,
    /// Additional upstream fields.
    #[serde(flatten)]
    pub extra: ExtensionFields,
}
impl std::fmt::Debug for LnurlPayerData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LnurlPayerData { [redacted] }")
    }
}

/// Payload for `LnurlPayParamsEvent`. Debug output redacts all payload fields.
#[derive(Clone, Serialize, Deserialize)]
pub struct LnurlPayParamsEvent {
    /// Minimum millisatoshis.
    pub min: i64,
    /// Maximum millisatoshis.
    pub max: i64,
    /// LNURL callback URL.
    pub callback: String,
    /// Invoice description.
    pub description: String,
    /// Maximum comment length.
    pub comment: i64,
    /// Supported payer data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payerdata: Option<LnurlPayerData>,
    /// Additional upstream fields.
    #[serde(flatten)]
    pub extra: ExtensionFields,
}
impl std::fmt::Debug for LnurlPayParamsEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LnurlPayParamsEvent { [redacted] }")
    }
}

/// Payload for `LnurlPaySuccessEvent`. Debug output redacts all payload fields.
#[derive(Clone, Serialize, Deserialize)]
pub struct LnurlPaySuccessEvent {
    /// Printable success action, when supplied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub success_action: Option<String>,
    /// Additional upstream fields.
    #[serde(flatten)]
    pub extra: ExtensionFields,
}
impl std::fmt::Debug for LnurlPaySuccessEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LnurlPaySuccessEvent { [redacted] }")
    }
}

/// Future notification method with its original params.
#[derive(Clone, Serialize, Deserialize)]
pub struct UnknownEvent {
    /// Unrecognized upstream method.
    pub method: String,
    /// Raw params, reserved for unknown methods only.
    pub params: Value,
}
impl std::fmt::Debug for UnknownEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("UnknownEvent { [redacted] }")
    }
}

/// All upstream notifications. Match known variants to access typed fields.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub enum ClicheEvent {
    /// Upstream `ready`.
    Ready(ReadyEvent),
    /// Upstream `payment_received`.
    PaymentReceived(PaymentReceivedEvent),
    /// Upstream `payment_succeeded`.
    PaymentSucceeded(PaymentSucceededEvent),
    /// Upstream `payment_failed`.
    PaymentFailed(PaymentFailedEvent),
    /// Upstream `lnurlpay_params`.
    LnurlPayParams(LnurlPayParamsEvent),
    /// Upstream `lnurlpay_success`.
    LnurlPaySuccess(LnurlPaySuccessEvent),
    /// A future method, not a malformed known event.
    Unknown(UnknownEvent),
}
impl ClicheEvent {
    /// The exact cliche method name.
    pub fn method(&self) -> &str {
        match self {
            Self::Ready(..) => "ready",
            Self::PaymentReceived(..) => "payment_received",
            Self::PaymentSucceeded(..) => "payment_succeeded",
            Self::PaymentFailed(..) => "payment_failed",
            Self::LnurlPayParams(..) => "lnurlpay_params",
            Self::LnurlPaySuccess(..) => "lnurlpay_success",
            Self::Unknown(event) => &event.method,
        }
    }

    /// Decode by method. Invalid known payloads return a safe diagnostic.
    pub fn decode(method: &str, params: Value) -> Result<Self, EventDecodeError> {
        let decoded = match method {
            "ready" => serde_json::from_value(params).map(Self::Ready),
            "payment_received" => serde_json::from_value(params).map(Self::PaymentReceived),
            "payment_succeeded" => serde_json::from_value(params).map(Self::PaymentSucceeded),
            "payment_failed" => serde_json::from_value(params).map(Self::PaymentFailed),
            "lnurlpay_params" => serde_json::from_value(params).map(Self::LnurlPayParams),
            "lnurlpay_success" => serde_json::from_value(params).map(Self::LnurlPaySuccess),
            _ => {
                return Ok(Self::Unknown(UnknownEvent {
                    method: method.into(),
                    params,
                }));
            }
        };
        // Serde errors can quote invalid values (including secrets). Never expose them.
        decoded.map_err(|_| EventDecodeError {
            method: method.into(),
            reason: DecodeReason::InvalidPayload,
        })
    }
}
impl Serialize for ClicheEvent {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut out = serializer.serialize_struct("ClicheEvent", 2)?;
        out.serialize_field("method", self.method())?;
        match self {
            Self::Ready(payload) => out.serialize_field("params", payload)?,
            Self::PaymentReceived(payload) => out.serialize_field("params", payload)?,
            Self::PaymentSucceeded(payload) => out.serialize_field("params", payload)?,
            Self::PaymentFailed(payload) => out.serialize_field("params", payload)?,
            Self::LnurlPayParams(payload) => out.serialize_field("params", payload)?,
            Self::LnurlPaySuccess(payload) => out.serialize_field("params", payload)?,
            Self::Unknown(event) => out.serialize_field("params", &event.params)?,
        }
        out.end()
    }
}
impl<'de> Deserialize<'de> for ClicheEvent {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            method: String,
            params: Value,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::decode(&wire.method, wire.params)
            .map_err(|_| serde::de::Error::custom("invalid event payload"))
    }
}

/// Safe notification decoding failure.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EventDecodeError {
    /// Upstream method, or empty for an invalid envelope.
    pub method: String,
    /// Failure category without raw payload values.
    pub reason: DecodeReason,
}
/// Why a notification was rejected.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecodeReason {
    /// Known method with missing or incorrectly typed fields.
    InvalidPayload,
    /// Invalid JSON or envelope.
    InvalidEnvelope,
}

/// Connection lifecycle state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    /// WebSocket open and command readiness verified.
    Connected,
    /// Connection lost; no commands are admitted.
    Disconnected,
    /// Waiting to reconnect; no command replay takes place.
    Reconnecting,
    /// Client permanently closed.
    Closed,
}
/// Why observations may be missing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GapReason {
    /// A bounded local receiver fell behind.
    SubscriberLagged,
    /// Upstream has no replay after connection loss.
    UpstreamDisconnected,
}
/// A gap in event observation. Reconcile business state before relying on it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EventGap {
    /// Source of loss.
    pub reason: GapReason,
    /// Known lost local record count; unknown for upstream disconnects.
    pub missed_count: Option<u64>,
}
/// Stream item with typed bridge records and typed upstream payloads.
#[non_exhaustive]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventRecord {
    /// Upstream notification.
    Notification {
        /// Typed upstream event, serialized as method and params.
        #[serde(flatten)]
        event: ClicheEvent,
    },
    /// Local connection state (also sent as a subscription's first record).
    Connection {
        /// Current state.
        state: ConnectionState,
    },
    /// Observable delivery loss.
    Gap {
        /// Loss details.
        #[serde(flatten)]
        gap: EventGap,
    },
    /// Malformed upstream notification; reconcile affected business state.
    InvalidNotification {
        /// Safe failure description.
        #[serde(flatten)]
        error: EventDecodeError,
    },
}

/// JSON-serializable subscription delivery. Sequence is local, not a replay offset.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EventDelivery {
    /// Current bridge envelope version.
    pub schema_version: u8,
    /// Opaque ID unique to this subscription.
    pub stream_id: String,
    /// Monotonic local sequence, including status/gap/decode records.
    pub sequence: u64,
    /// Typed content.
    #[serde(flatten)]
    pub record: EventRecord,
}

/// Independent bounded subscription. Slow consumers get explicit gap records.
pub struct Subscription {
    pub(crate) receiver: tokio::sync::broadcast::Receiver<EventRecord>,
    pub(crate) initial: Option<ConnectionState>,
    pub(crate) status: tokio::sync::watch::Receiver<ConnectionState>,
    pub(crate) stream_id: String,
    pub(crate) sequence: u64,
    pub(crate) ended: bool,
    pub(crate) methods: Vec<String>,
}
impl Subscription {
    /// Restrict upstream methods. Bridge records always bypass this filter.
    pub fn with_methods(mut self, methods: impl IntoIterator<Item = String>) -> Self {
        self.methods = methods.into_iter().collect();
        self
    }
    /// Next typed record, or `None` after final close. Cancellation-safe.
    pub async fn recv(&mut self) -> Option<EventDelivery> {
        if self.ended {
            return None;
        }
        let record = if let Some(state) = self.initial.take() {
            EventRecord::Connection { state }
        } else {
            loop {
                let record = match self.receiver.recv().await {
                    Ok(record) => record,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                        EventRecord::Gap {
                            gap: EventGap {
                                reason: GapReason::SubscriberLagged,
                                missed_count: Some(count),
                            },
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        EventRecord::Connection {
                            state: ConnectionState::Closed,
                        }
                    }
                };
                if let EventRecord::Notification { event } = &record
                    && !self.methods.is_empty()
                    && !self.methods.iter().any(|m| m == event.method())
                {
                    continue;
                }
                // If a terminal status itself was lost to lag, still end after
                // draining retained records instead of waiting on a live sender.
                if self.receiver.is_empty() && *self.status.borrow() == ConnectionState::Closed {
                    self.initial = Some(ConnectionState::Closed);
                }
                break record;
            }
        };
        if matches!(
            record,
            EventRecord::Connection {
                state: ConnectionState::Closed
            }
        ) {
            self.ended = true;
        }
        self.sequence = self.sequence.checked_add(1)?;
        Some(EventDelivery {
            schema_version: 1,
            stream_id: self.stream_id.clone(),
            sequence: self.sequence,
            record,
        })
    }
}
