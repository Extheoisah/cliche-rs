//! Managed processes use an isolated copy of this test binary as fake cliche.
#![cfg(all(feature = "process", unix))]
use cliche_client::{
    ErrorKind,
    process::{ManagedCliche, ProcessConfig},
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::{io::Write, os::unix::fs::PermissionsExt, time::Duration};
use tokio_tungstenite::tungstenite::Message;

#[test]
fn fake_child() {
    if std::env::var("CLICHE_TEST_FIXTURE").as_deref() != Ok("1") {
        return;
    }
    let mode = std::env::var("CLICHE_TEST_MODE").unwrap();
    if mode == "exit" {
        std::process::exit(19);
    }
    if mode == "hang" {
        loop {
            std::thread::park();
        }
    }
    // More than a pipe buffer on both streams: readiness requires active draining.
    for _ in 0..32 {
        std::io::stdout().write_all(&[b'x'; 8192]).unwrap();
        std::io::stderr().write_all(&[b'y'; 8192]).unwrap();
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let port = std::env::var("CLICHE_TEST_PORT").unwrap();
        let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}"))
            .await
            .unwrap();
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
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
        }
    });
}
fn config(mode: &str, dir: &tempfile::TempDir) -> ProcessConfig {
    let exe = std::env::current_exe().unwrap();
    let quoted = exe.display().to_string().replace('\'', "'\\''");
    let script = format!(
        "#!/bin/sh\ntrap '' TERM\nexport CLICHE_TEST_FIXTURE=1\nexport CLICHE_TEST_MODE={mode}\nfor arg in \"$@\"; do\ncase \"$arg\" in -Dcliche.websocket.port=*) export CLICHE_TEST_PORT=\"${{arg#*=}}\";; esac\ndone\nexec '{quoted}' --exact fake_child --nocapture\n"
    );
    let path = dir.path().join("fake-cliche");
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let mut config = ProcessConfig::native(path, dir.path().join("data"));
    config.port = port;
    config.startup_timeout = Duration::from_secs(3);
    config.shutdown_grace = Duration::from_millis(50);
    config
}
async fn wait_unlocked(data_dir: &std::path::Path) {
    use fs2::FileExt;
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Ok(file) = std::fs::File::open(data_dir.join(".cliche-client.lock"))
                && file.try_lock_exclusive().is_ok()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn starts_drains_keeps_stdin_and_stops_only_owned_child() {
    let dir = tempfile::tempdir().unwrap();
    let config = config("ready", &dir);
    let data_dir = config.data_dir.clone();
    let managed = ManagedCliche::start(config.clone()).await.unwrap();
    assert_eq!(managed.client().ping().await.unwrap()["ping"], "pong");
    assert!(ManagedCliche::start(config).await.is_err());
    managed.shutdown().await;
    wait_unlocked(&data_dir).await;
}
#[tokio::test]
async fn startup_exit_timeout_and_cancellation_release_ownership() {
    for mode in ["exit", "hang"] {
        let dir = tempfile::tempdir().unwrap();
        let mut config = config(mode, &dir);
        config.startup_timeout = Duration::from_millis(100);
        let data_dir = config.data_dir.clone();
        let error = match ManagedCliche::start(config).await {
            Ok(_) => panic!("unexpected ready"),
            Err(e) => e,
        };
        assert_eq!(error.kind, ErrorKind::Process);
        wait_unlocked(&data_dir).await;
    }
    let dir = tempfile::tempdir().unwrap();
    let config = config("hang", &dir);
    let data_dir = config.data_dir.clone();
    let task = tokio::spawn(ManagedCliche::start(config));
    tokio::time::timeout(Duration::from_secs(1), async {
        while !data_dir.join(".cliche-client.lock").exists() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    task.abort();
    let _ = task.await;
    wait_unlocked(&data_dir).await;
}
#[tokio::test]
async fn busy_port_is_never_adopted() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = config("ready", &dir);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    config.port = listener.local_addr().unwrap().port();
    assert!(ManagedCliche::start(config).await.is_err());
    assert!(listener.local_addr().is_ok());
}
