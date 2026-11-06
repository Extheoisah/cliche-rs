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
