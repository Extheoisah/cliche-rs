//! Typed subscriptions. Prints only selected public payment fields.
use cliche_client::{
    Client,
    events::{ClicheEvent, EventRecord},
};
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let endpoint = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "ws://127.0.0.1:12000/".into());
    let client = Client::connect(endpoint).await?;
    let mut events = client.subscribe();
    while let Some(delivery) = events.recv().await {
        match delivery.record {
            EventRecord::Notification {
                event: ClicheEvent::PaymentReceived(payment),
            } => {
                println!(
                    "received hash={} msat={:?}",
                    payment.payment_hash, payment.msatoshi
                );
            }
            EventRecord::Gap { .. } => eprintln!("event gap: reconcile persisted payment hashes"),
            EventRecord::InvalidNotification { .. } => {
                eprintln!("invalid notification: reconcile affected payment state")
            }
            _ => {}
        }
    }
    Ok(())
}
