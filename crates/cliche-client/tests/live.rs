//! Explicit opt-in, read-only compatibility checks. Never run against a wallet by default.
use cliche_client::Client;
#[tokio::test]
#[ignore = "requires explicitly configured CLICHE_TEST_URL and an operator-managed cliche"]
async fn read_only_live_cliche_smoke() {
    let endpoint = std::env::var("CLICHE_TEST_URL").expect("set CLICHE_TEST_URL explicitly");
    let mut config = Client::builder(endpoint);
    if let Ok(pubkey) = std::env::var("CLICHE_TEST_PUBKEY") {
        config = config.expected_pubkey(pubkey);
    }
    if let Ok(chain) = std::env::var("CLICHE_TEST_CHAIN") {
        config = config.expected_chain(chain);
    }
    let client = config.connect().await.expect("cliche readiness failed");
    assert_eq!(client.ping().await.unwrap()["ping"], "pong");
    let info = client.get_info().await.unwrap();
    assert!(info["main_pubkey"].is_string());
    assert!(info["chain"].is_string());
    client.close().await;
}
#[cfg(feature = "process")]
#[tokio::test]
#[ignore = "requires dedicated disposable Nix cliche data, seed configured out of band"]
async fn managed_nix_smoke() {
    use cliche_client::process::{ManagedCliche, ProcessConfig};
    let executable = std::env::var("CLICHE_TEST_BINARY").expect("set CLICHE_TEST_BINARY");
    let data = std::env::var("CLICHE_TEST_DATA_DIR").expect("set dedicated CLICHE_TEST_DATA_DIR");
    let mut config = ProcessConfig::native(executable, data);
    config.port = std::env::var("CLICHE_TEST_PORT")
        .expect("set dedicated CLICHE_TEST_PORT")
        .parse()
        .unwrap();
    config.network = std::env::var("CLICHE_TEST_NETWORK").expect("set CLICHE_TEST_NETWORK");
    assert_ne!(
        config.network, "mainnet",
        "managed smoke must use disposable non-mainnet data"
    );
    let managed = ManagedCliche::start(config)
        .await
        .expect("Nix cliche startup failed");
    assert_eq!(managed.client().ping().await.unwrap()["ping"], "pong");
    managed.shutdown().await;
}

#[tokio::test]
#[ignore = "requires explicitly configured CLICHE_TEST_URL and an operator-managed cliche"]
async fn read_only_live_concurrent_clients() {
    use cliche_client::events::{ConnectionState, EventRecord};
    let endpoint = std::env::var("CLICHE_TEST_URL").expect("set CLICHE_TEST_URL explicitly");
    let first = Client::connect(&endpoint)
        .await
        .expect("first client readiness failed");
    let info = first.get_info().await.expect("get-info failed");
    // Pins are observed from the first connection: this tests consistent identity,
    // not independent verification of binary or wallet provenance.
    let pubkey = info["main_pubkey"].as_str().expect("missing main_pubkey");
    let chain = info["chain"].as_str().expect("missing chain");
    let second = Client::builder(endpoint)
        .expected_pubkey(pubkey)
        .expected_chain(chain)
        .connect()
        .await
        .expect("second client identity check failed");
    let mut events = second.subscribe();
    assert!(matches!(
        events.recv().await.map(|d| d.record),
        Some(EventRecord::Connection {
            state: ConnectionState::Connected
        })
    ));
    let mut tasks = Vec::new();
    for index in 0..32 {
        let client = if index % 2 == 0 {
            first.clone()
        } else {
            second.clone()
        };
        tasks.push(tokio::spawn(async move {
            assert_eq!(
                client.ping().await.expect("concurrent ping failed")["ping"],
                "pong"
            );
        }));
    }
    for task in tasks {
        task.await.expect("ping task failed");
    }
    second.close().await;
    // Natural notifications may already be queued; do not assume an idle wallet.
    let mut closed = false;
    while let Some(delivery) = events.recv().await {
        if matches!(
            delivery.record,
            EventRecord::Connection {
                state: ConnectionState::Closed
            }
        ) {
            closed = true;
            break;
        }
    }
    assert!(closed);
    // Closing one attached client must leave the other client and cliche usable.
    assert_eq!(
        first.ping().await.expect("surviving client ping failed")["ping"],
        "pong"
    );
    first.close().await;
}
