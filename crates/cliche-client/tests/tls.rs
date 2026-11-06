//! Local TLS trust verification; does not alter Rustls's global crypto provider.
#![cfg(feature = "tls")]
use cliche_client::Client;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn private_trust_connects_and_default_trust_rejects_unknown_certificate() {
    let generated = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let certificate = generated.cert.der().clone();
    let key = rustls::pki_types::PrivatePkcs8KeyDer::from(generated.signing_key.serialize_der());
    let server_config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(vec![certificate.clone()], key.into())
    .unwrap();
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_config));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("wss://localhost:{}/", listener.local_addr().unwrap().port());
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        assert!(acceptor.accept(stream).await.is_err()); // Untrusted default roots.
        let (stream, _) = listener.accept().await.unwrap();
        let stream = acceptor.accept(stream).await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        while let Some(Ok(Message::Text(text))) = socket.next().await {
            let req: Value = serde_json::from_str(&text).unwrap();
            socket
                .send(Message::Text(
                    json!({"id":req["id"],"result":{"ping":"pong"}})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
        }
    });
    assert!(Client::connect(&endpoint).await.is_err());
    let mut roots = rustls::RootCertStore::empty();
    roots.add(certificate).unwrap();
    let tls = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_root_certificates(roots)
    .with_no_client_auth();
    let client = Client::builder(endpoint)
        .tls_config(Arc::new(tls))
        .connect()
        .await
        .unwrap();
    assert_eq!(client.ping().await.unwrap()["ping"], "pong");
    client.close().await;
    server.await.unwrap();
}
