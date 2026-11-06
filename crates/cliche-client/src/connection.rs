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
    let operation = async {
        let limits = WebSocketConfig::default()
            .max_message_size(Some(config.max_message_size))
            .max_frame_size(Some(config.max_message_size));
        let (mut socket, _) =
            tokio_tungstenite::connect_async_with_config(&config.endpoint, Some(limits), false)
                .await
                .map_err(|_| {
                    Error::new(
                        ErrorKind::Connection,
                        "websocket handshake failed",
                        Execution::NotSent,
                    )
                })?;
        let id = uuid::Uuid::new_v4().to_string();
        socket
            .send(Message::Text(
                json!({"jsonrpc":"2.0","id":id,"method":"ping","params":{}})
                    .to_string()
                    .into(),
            ))
            .await
            .map_err(|_| {
                Error::new(
                    ErrorKind::Connection,
                    "readiness write failed",
                    Execution::NotSent,
                )
            })?;
        while let Some(frame) = socket.next().await {
            match frame {
                Ok(Message::Text(text)) => {
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
                        return Ok(socket);
                    }
                }
                Ok(Message::Ping(_)) => {
                    socket.flush().await.map_err(|_| {
                        Error::new(ErrorKind::Connection, "pong failed", Execution::NotSent)
                    })?;
                }
                Ok(Message::Pong(_)) => {}
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
                "connect/readiness deadline exceeded",
                Execution::NotSent,
            )
        })?
}

pub(crate) async fn run(
    socket: Socket,
    mut commands: mpsc::Receiver<Outbound>,
    state: Arc<State>,
    mut stop: watch::Receiver<bool>,
    done: watch::Sender<bool>,
    _config: ClientBuilder,
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
                    Incoming::Reply(id, result) => { if let Some(pending) = state.lock().pending.remove(&id) { let _ = pending.reply.send(result); } }
                    Incoming::Event(record) => { let _ = state.events.send(record); }
                    Incoming::Invalid => { let _ = state.events.send(EventRecord::InvalidNotification { error: crate::events::EventDecodeError { method: String::new(), reason: crate::events::DecodeReason::InvalidEnvelope } }); }
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
    state.disconnect(ErrorKind::Connection);
    state.set_status(ConnectionState::Closed);
    done.send_replace(true);
}
