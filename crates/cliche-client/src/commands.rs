//! Common cliche commands. Results remain JSON; notifications are typed.
use crate::{Client, Error};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Invoice creation parameters. Secret-bearing values are not Debug-printable.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct CreateInvoiceParams {
    /// Amount in millisatoshis. Omit for an amountless invoice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msatoshi: Option<i64>,
    /// Human-readable description, mutually exclusive with description_hash.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// SHA256 description hash in hexadecimal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description_hash: Option<String>,
    /// Optional secret 32-byte preimage in hexadecimal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preimage: Option<String>,
    /// Optional application label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}
/// Payment initiation parameters. Success is not confirmation of settlement.
#[derive(Clone, Serialize, Deserialize)]
pub struct PayInvoiceParams {
    /// BOLT11 invoice.
    pub invoice: String,
    /// Millisatoshis for an amountless invoice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msatoshi: Option<i64>,
}
fn positive(amount: Option<i64>) -> Result<(), Error> {
    if amount.is_some_and(|amount| amount <= 0) {
        return Err(Error::invalid("amount must be positive millisatoshis"));
    }
    Ok(())
}
fn hash(value: &str) -> Result<(), Error> {
    if value.len() != 64 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::invalid("expected 32-byte hexadecimal value"));
    }
    Ok(())
}
fn params(value: impl Serialize) -> Result<Value, Error> {
    serde_json::to_value(value).map_err(|_| Error::invalid("parameters could not be serialized"))
}
impl Client {
    /// Application-level liveness. Does not imply channel/payment readiness.
    pub async fn ping(&self) -> Result<Value, Error> {
        self.call("ping", json!({})).await
    }
    /// Node, wallet, channel, and chain information. May contain sensitive data.
    pub async fn get_info(&self) -> Result<Value, Error> {
        self.call("get-info", json!({})).await
    }
    /// Create an invoice. Requires upstream receiving capacity.
    pub async fn create_invoice(&self, request: CreateInvoiceParams) -> Result<Value, Error> {
        positive(request.msatoshi)?;
        if request.description.is_some() && request.description_hash.is_some() {
            return Err(Error::invalid(
                "description and description_hash are mutually exclusive",
            ));
        }
        if let Some(value) = &request.description_hash {
            hash(value)?;
        }
        if let Some(value) = &request.preimage {
            hash(value)?;
        }
        self.call("create-invoice", params(request)?).await
    }
    /// Initiate payment. Observe typed payment events or check_payment for final state.
    pub async fn pay_invoice(&self, request: PayInvoiceParams) -> Result<Value, Error> {
        positive(request.msatoshi)?;
        if request.invoice.trim().is_empty() {
            return Err(Error::invalid("invoice is empty"));
        }
        self.call("pay-invoice", params(request)?).await
    }
    /// Reconcile a known payment hash after a timeout, missed event, or reconnect.
    pub async fn check_payment(&self, payment_hash: &str) -> Result<Value, Error> {
        hash(payment_hash)?;
        self.call("check-payment", json!({"hash":payment_hash}))
            .await
    }
    /// Recent payments. None uses cliche's default (5), not complete history.
    pub async fn list_payments(&self, count: Option<u32>) -> Result<Value, Error> {
        if count.is_some_and(|n| n == 0 || n > i32::MAX as u32) {
            return Err(Error::invalid("count must be between 1 and i32::MAX"));
        }
        let request = count.map_or_else(|| json!({}), |count| json!({"count":count}));
        self.call("list-payments", request).await
    }
    /// Request the node's on-chain receiving address.
    pub async fn get_address(&self) -> Result<Value, Error> {
        self.call("get-address", json!({})).await
    }
}
