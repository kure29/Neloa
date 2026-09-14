use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use neloa_relay::{router, RelayState};
use neloa_relay_protocol::{
    decode_tunnel_frame, encode_tunnel_frame, ClientControl, RelayDevice, ServerControl,
    RELAY_PROTOCOL_VERSION,
};
use tokio::{net::TcpListener, time::timeout};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, http::header::AUTHORIZATION, Message},
    MaybeTlsStream, WebSocketStream,
};
use uuid::Uuid;

const TOKEN: &str = "test-only-relay-token-32-characters";
type ClientSocket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

fn device(id: &str, name: &str) -> RelayDevice {
    RelayDevice {
        id: id.into(),
        name: name.into(),
        platform: "test".into(),
        app_version: "0.1.9".into(),
        protocol_version: 1,
        min_protocol_version: 1,
        capabilities: vec!["noise-xx".into(), "file-transfer".into()],
    }
}

async fn connect(url: &str) -> ClientSocket {
    let mut request = url.into_client_request().unwrap();
    request
        .headers_mut()
        .insert(AUTHORIZATION, format!("Bearer {TOKEN}").parse().unwrap());
    connect_async(request).await.unwrap().0
}

async fn send_control(socket: &mut ClientSocket, control: &ClientControl) {
    socket
        .send(Message::Text(
            serde_json::to_string(control).unwrap().into(),
        ))
        .await
        .unwrap();
}

async fn receive_control(
    socket: &mut ClientSocket,
    predicate: impl Fn(&ServerControl) -> bool,
) -> ServerControl {
    timeout(Duration::from_secs(2), async {
        loop {
            let message = socket.next().await.unwrap().unwrap();
            if let Message::Text(text) = message {
                let control = serde_json::from_str::<ServerControl>(&text).unwrap();
                if predicate(&control) {
                    return control;
                }
            }
        }
    })
    .await
    .expect("timed out waiting for relay control message")
}

#[tokio::test]
async fn authenticated_clients_open_and_forward_an_opaque_tunnel() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, router(RelayState::new(TOKEN).unwrap()))
            .await
            .unwrap();
    });
    let url = format!("ws://{address}/v1/ws");

    let mut first = connect(&url).await;
    send_control(
        &mut first,
        &ClientControl::Register {
            relay_protocol_version: RELAY_PROTOCOL_VERSION,
            device: device("first", "First"),
        },
    )
    .await;
    receive_control(&mut first, |message| {
        matches!(message, ServerControl::Registered { .. })
    })
    .await;

    let mut second = connect(&url).await;
    send_control(
        &mut second,
        &ClientControl::Register {
            relay_protocol_version: RELAY_PROTOCOL_VERSION,
            device: device("second", "Second"),
        },
    )
    .await;
    receive_control(&mut second, |message| {
        matches!(message, ServerControl::Registered { .. })
    })
    .await;

    let tunnel_id = Uuid::new_v4();
    send_control(
        &mut first,
        &ClientControl::OpenTunnel {
            tunnel_id,
            target_id: "second".into(),
        },
    )
    .await;
    receive_control(&mut first, |message| {
        matches!(message, ServerControl::TunnelOpened { tunnel_id: id, .. } if *id == tunnel_id)
    })
    .await;
    receive_control(&mut second, |message| {
        matches!(message, ServerControl::IncomingTunnel { tunnel_id: id, .. } if *id == tunnel_id)
    })
    .await;

    let opaque_payload = b"opaque noise ciphertext";
    first
        .send(Message::Binary(
            encode_tunnel_frame(tunnel_id, opaque_payload)
                .unwrap()
                .into(),
        ))
        .await
        .unwrap();
    let received = timeout(Duration::from_secs(2), async {
        loop {
            if let Message::Binary(frame) = second.next().await.unwrap().unwrap() {
                return frame;
            }
        }
    })
    .await
    .expect("timed out waiting for relayed binary frame");
    let (received_tunnel, received_payload) = decode_tunnel_frame(&received).unwrap();
    assert_eq!(received_tunnel, tunnel_id);
    assert_eq!(received_payload, opaque_payload);

    send_control(&mut second, &ClientControl::CloseTunnel { tunnel_id }).await;
    receive_control(&mut first, |message| {
        matches!(message, ServerControl::TunnelClosed { tunnel_id: id, .. } if *id == tunnel_id)
    })
    .await;
    server.abort();
}

#[tokio::test]
async fn websocket_endpoint_rejects_missing_bearer_token() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, router(RelayState::new(TOKEN).unwrap()))
            .await
            .unwrap();
    });

    let error = connect_async(format!("ws://{address}/v1/ws"))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("401"));
    server.abort();
}
