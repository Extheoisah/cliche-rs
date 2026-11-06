//! Read-only attachment. Output may contain wallet-sensitive information.
use cliche_client::Client;
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let endpoint = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "ws://127.0.0.1:12000/".into());
    let client = Client::connect(endpoint).await?;
    println!("{}", client.get_info().await?);
    client.close().await;
    Ok(())
}
