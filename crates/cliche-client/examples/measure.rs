//! Reproducible client + mock-server memory/throughput baseline; never uses a wallet.
use cliche_client::{Client, events::EventRecord};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::Message;
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let idle = std::env::args().any(|arg| arg == "--idle");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = format!("ws://{}/", listener.local_addr()?);
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await?;
        let mut socket = tokio_tungstenite::accept_async(stream).await?;
        while let Some(Ok(Message::Text(text))) = socket.next().await {
            let req: Value = serde_json::from_str(&text)?;
            let result = if req["method"] == "ping" {
                json!({"ping":"pong"})
            } else {
                req["params"].clone()
            };
            if req["method"] != "ping" {
                socket
                    .send(Message::Text(
                        json!({"method":"measure","params":result})
                            .to_string()
                            .into(),
                    ))
                    .await?;
            }
            socket
                .send(Message::Text(
                    json!({"id":req["id"],"result":result}).to_string().into(),
                ))
                .await?;
        }
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    });
    let client = Client::connect(endpoint).await?;
    let mut slow = client.subscribe();
    slow.recv().await;
    let start = Instant::now();
    if idle {
        tokio::time::sleep(Duration::from_millis(250)).await;
    } else {
        let mut tasks = Vec::new();
        for _ in 0..128 {
            let client = client.clone();
            tasks.push(tokio::spawn(async move {
                for _ in 0..100 {
                    client
                        .call("echo", json!({"payload":"x".repeat(1024)}))
                        .await?;
                }
                Ok::<_, cliche_client::Error>(())
            }));
        }
        for task in tasks {
            task.await??;
        }
        assert!(matches!(
            slow.recv().await.map(|d| d.record),
            Some(EventRecord::Gap { .. })
        ));
    }
    println!(
        "mode={} calls={} elapsed_ms={}",
        if idle { "idle" } else { "load" },
        if idle { 0 } else { 12800 },
        start.elapsed().as_millis()
    );
    client.close().await;
    server.await?.map_err(|_| "mock server failed")?;
    Ok(())
}
