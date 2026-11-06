use crate::{
    Error, ErrorKind, Execution,
    client::{ClientBuilder, Outbound, State},
    events::{ConnectionState, EventRecord},
    protocol::{self, Incoming},
};
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use std::sync::Arc;
use tokio::{
    net::TcpStream,
    sync::{mpsc, watch},
    time::timeout,
};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream,
    tungstenite::{Message, protocol::WebSocketConfig},
};
type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub(crate) async fn open(config: &ClientBuilder) -> Result<Socket, Error> {
    let stage = std::sync::atomic::AtomicU8::new(0);
    let operation = async {
        let limits = WebSocketConfig::default()
            .max_message_size(Some(config.max_message_size))
            .max_frame_size(Some(config.max_message_size));
        #[cfg(feature = "tls")]
        let connected = {
            let connector = if config.endpoint.starts_with("wss://") {
                let tls = match &config.tls_config {
                    Some(config) => config.clone(),
                    None => Arc::new(
                        rustls::ClientConfig::builder_with_provider(Arc::new(
                            rustls::crypto::ring::default_provider(),
                        ))
                        .with_safe_default_protocol_versions()
                        .map_err(|_| Error::invalid("TLS protocol configuration failed"))?
                        .with_root_certificates(rustls::RootCertStore::from_iter(
                            webpki_roots::TLS_SERVER_ROOTS.iter().cloned(),
                        ))
                        .with_no_client_auth(),
                    ),
                };
                Some(tokio_tungstenite::Connector::Rustls(tls))
            } else {
                None
            };
            tokio_tungstenite::connect_async_tls_with_config(
                &config.endpoint,
                Some(limits),
                false,
                connector,
            )
            .await
        };
        #[cfg(not(feature = "tls"))]
        let connected =
            tokio_tungstenite::connect_async_with_config(&config.endpoint, Some(limits), false)
                .await;
        let (mut socket, _) = connected.map_err(|_| {
            Error::new(
                ErrorKind::Connection,
                "websocket handshake failed",
                Execution::NotSent,
            )
        })?;
        stage.store(1, std::sync::atomic::Ordering::Relaxed);
        let id = uuid::Uuid::new_v4().to_string();
        let probe = Message::Text(
            json!({"jsonrpc":"2.0","id":id,"method":"ping","params":{}})
                .to_string()
                .into(),
        );
        // cliche begins consuming input and subscribing to its output topic
        // independently. An immediate reply can be published before this socket
        // subscribes. Only this internal, side-effect-free readiness probe is
        // repeated, bounded by connect_timeout; application calls are never retried.
        let mut retry = tokio::time::interval(std::time::Duration::from_millis(250));
        retry.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            let frame = tokio::select! {
                _ = retry.tick() => {
                    socket.send(probe.clone()).await.map_err(|_| {
                        Error::new(ErrorKind::Connection, "readiness write failed", Execution::NotSent)
                    })?;
                    continue;
                }
                frame = socket.next() => frame,
            };
            match frame {
                Some(Ok(Message::Text(text))) => {
                    if let Incoming::Reply(reply_id, result) = protocol::parse(&text)
                        && reply_id == id
                    {
                        if result?["ping"] != "pong" {
                            return Err(Error::new(
                                ErrorKind::Protocol,
                                "unexpected readiness response",
                                Execution::NotSent,
                            ));
                        }
                        if config.expected_pubkey.is_some() || config.expected_chain.is_some() {
                            stage.store(2, std::sync::atomic::Ordering::Relaxed);
                            verify_identity(&mut socket, config).await?;
                        }
                        return Ok(socket);
                    }
                }
                Some(Ok(Message::Ping(_))) => {
                    socket.flush().await.map_err(|_| {
                        Error::new(ErrorKind::Connection, "pong failed", Execution::NotSent)
                    })?;
                }
                Some(Ok(Message::Pong(_))) => {}
                _ => break,
            }
        }
        Err(Error::new(
            ErrorKind::Connection,
            "socket closed during readiness",
            Execution::NotSent,
        ))
    };
    timeout(config.connect_timeout, operation)
        .await
        .map_err(|_| {
            Error::new(
                ErrorKind::Timeout,
                match stage.load(std::sync::atomic::Ordering::Relaxed) {
                    0 => "websocket handshake deadline exceeded",
                    1 => "command readiness deadline exceeded",
                    _ => "identity check deadline exceeded",
                },
                Execution::NotSent,
            )
        })?
}

async fn verify_identity(socket: &mut Socket, config: &ClientBuilder) -> Result<(), Error> {
    let id = uuid::Uuid::new_v4().to_string();
    socket
        .send(Message::Text(
            json!({"jsonrpc":"2.0","id":id,"method":"get-info","params":{}})
                .to_string()
                .into(),
        ))
        .await
        .map_err(|_| {
            Error::new(
                ErrorKind::Connection,
                "identity check write failed",
                Execution::NotSent,
            )
        })?;
    while let Some(Ok(frame)) = socket.next().await {
        if let Message::Text(text) = frame
            && let Incoming::Reply(reply_id, result) = protocol::parse(&text)
            && reply_id == id
        {
            let info = result?;
            if config
                .expected_pubkey
                .as_ref()
                .is_some_and(|expected| info["main_pubkey"].as_str() != Some(expected))
                || config
                    .expected_chain
                    .as_ref()
                    .is_some_and(|expected| info["chain"].as_str() != Some(expected))
            {
                return Err(Error::new(
                    ErrorKind::Protocol,
                    "cliche identity mismatch",
                    Execution::NotSent,
                ));
            }
            return Ok(());
        }
    }
    Err(Error::new(
        ErrorKind::Connection,
        "identity check disconnected",
        Execution::NotSent,
    ))
}

pub(crate) async fn run(
    mut socket: Socket,
    mut commands: mpsc::Receiver<Outbound>,
    state: Arc<State>,
    mut stop: watch::Receiver<bool>,
    done: watch::Sender<bool>,
    config: ClientBuilder,
) {
    loop {
        session(socket, commands, state.clone(), &mut stop).await;
        state.disconnect(ErrorKind::Connection);
        if *stop.borrow() {
            break;
        }
        state.set_status(ConnectionState::Disconnected);
        let _ = state.events.send(EventRecord::Gap {
            gap: crate::events::EventGap {
                reason: crate::events::GapReason::UpstreamDisconnected,
                missed_count: None,
            },
        });
        let Some(policy) = config.reconnect else {
            break;
        };
        let mut connected = None;
        state.set_status(ConnectionState::Reconnecting);
        for attempt in 0..policy.max_attempts {
            let delay = policy
                .initial_delay
                .saturating_mul(2_u32.saturating_pow(attempt))
                .min(policy.max_delay);
            let jitter = f64::from(uuid::Uuid::new_v4().as_bytes()[0]) / 1024.0;
            tokio::select! {
                biased;
                _ = stop.wait_for(|stop| *stop) => break,
                _ = tokio::time::sleep(delay.mul_f64(1.0 + jitter)) => {},
            }
            let result = tokio::select! {
                biased;
                _ = stop.wait_for(|stop| *stop) => break,
                result = open(&config) => result,
            };
            match result {
                Ok(next) => {
                    connected = Some(next);
                    break;
                }
                Err(error)
                    if error.kind == ErrorKind::Protocol
                        || error.kind == ErrorKind::InvalidArgument =>
                {
                    break;
                }
                Err(_) => {}
            }
        }
        let Some(next) = connected else {
            break;
        };
        if state.stopping.load(std::sync::atomic::Ordering::SeqCst) {
            break;
        }
        let (tx, rx) = mpsc::channel(config.max_in_flight);
        state.lock().active = Some(crate::client::Active {
            writer: tx,
            session: uuid::Uuid::new_v4().to_string(),
            counter: 0,
        });
        socket = next;
        commands = rx;
        state.set_status(ConnectionState::Connected);
    }
    state.set_status(ConnectionState::Closed);
    done.send_replace(true);
}

async fn session(
    socket: Socket,
    mut commands: mpsc::Receiver<Outbound>,
    state: Arc<State>,
    stop: &mut watch::Receiver<bool>,
) {
    let (mut sink, mut stream) = socket.split();
    let write_state = state.clone();
    let mut writer = tokio::spawn(async move {
        while let Some(command) = commands.recv().await {
            {
                let mut shared = write_state.lock();
                let Some(pending) = shared.pending.get_mut(&command.id) else {
                    continue;
                };
                if command.deadline <= tokio::time::Instant::now() {
                    continue;
                }
                pending.sent = true;
            }
            match tokio::time::timeout_at(
                command.deadline,
                sink.send(Message::Text(command.text.into())),
            )
            .await
            {
                Ok(Ok(())) => {}
                _ => return,
            }
        }
    });
    let mut writer_completed = false;
    loop {
        tokio::select! {
            biased;
            _ = stop.wait_for(|stop| *stop) => break,
            _ = &mut writer => { writer_completed = true; break; },
            frame = stream.next() => match frame {
                Some(Ok(Message::Text(text))) => match protocol::parse(&text) {
                    Incoming::Reply(id, result) => {
                        if let Some(pending) = state.lock().pending.remove(&id) {
                            let _ = pending.reply.send(result);
                        }
                    }
                    Incoming::Event(record) => {
                        let _ = state.events.send(record);
                    }
                    Incoming::Invalid => {
                        let error = crate::events::EventDecodeError {
                            method: String::new(),
                            reason: crate::events::DecodeReason::InvalidEnvelope,
                        };
                        let _ = state.events.send(EventRecord::InvalidNotification { error });
                    }
                },
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => {},
                _ => break,
            }
        }
    }
    if !writer_completed {
        writer.abort();
        let _ = writer.await;
    }
}
