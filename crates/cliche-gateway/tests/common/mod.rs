use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{WebSocketStream, tungstenite::Message};
pub async fn listener() -> (TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("ws://{}/", listener.local_addr().unwrap());
    (listener, endpoint)
}
pub async fn accept(listener: &TcpListener) -> WebSocketStream<TcpStream> {
    let (socket, _) = listener.accept().await.unwrap();
    let mut socket = tokio_tungstenite::accept_async(socket).await.unwrap();
    let ping = request(&mut socket).await;
    assert_eq!(ping["method"], "ping");
    respond(&mut socket, &ping, json!({"ping":"pong"})).await;
    socket
}
pub async fn request(socket: &mut WebSocketStream<TcpStream>) -> Value {
    let message = socket.next().await.unwrap().unwrap();
    serde_json::from_str(message.to_text().unwrap()).unwrap()
}
pub async fn respond(socket: &mut WebSocketStream<TcpStream>, request: &Value, result: Value) {
    send(
        socket,
        json!({"jsonrpc":"2.0","id":request["id"],"result":result}),
    )
    .await;
}
pub async fn send(socket: &mut WebSocketStream<TcpStream>, value: Value) {
    socket
        .send(Message::Text(value.to_string().into()))
        .await
        .unwrap();
}
