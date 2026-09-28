#![allow(clippy::result_large_err)] // tungstenite's handshake callback returns its own error type

//! Live updates against local fakes of both channels: UwULock's realtime
//! WebSocket and Bitwarden's SignalR hub, on 127.0.0.1:0.

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::Message;
use uwulock_bitwarden::live::{Channel, Event, Hub, Realtime};
use uwulock_bitwarden::Server;

async fn listen() -> (TcpListener, Server) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = Server::self_hosted(&format!("http://localhost:{port}")).unwrap();
    (listener, server)
}

async fn text_of(socket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>) -> Value {
    loop {
        match socket.next().await.unwrap().unwrap() {
            Message::Text(text) => return serde_json::from_str(&text).unwrap(),
            _ => continue,
        }
    }
}

#[tokio::test]
async fn realtime_channel_announces_changes_and_takes_a_fresh_token() {
    let (listener, server) = listen().await;
    let fake = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut path = String::new();
        let mut socket = tokio_tungstenite::accept_hdr_async(
            stream,
            |request: &Request, mut response: Response| {
                path = request.uri().path().to_string();
                assert!(request.uri().query().is_none(), "no token in the URL");
                assert_eq!(
                    request.headers()["Sec-WebSocket-Protocol"],
                    "uwu.realtime.v1"
                );
                response
                    .headers_mut()
                    .insert("Sec-WebSocket-Protocol", "uwu.realtime.v1".parse().unwrap());
                Ok(response)
            },
        )
        .await
        .unwrap();
        let auth = text_of(&mut socket).await;
        assert_eq!(
            auth,
            json!({ "type": "auth", "token": "t1", "cursor": "c9" })
        );
        let ready = json!({ "type": "ready", "connectionId": "c0d1", "expires": 1790000000u64, "heartbeat": 25 });
        socket
            .send(Message::Text(ready.to_string().into()))
            .await
            .unwrap();
        let changed = json!({ "type": "changed", "areas": ["vault", "uwu"] });
        socket
            .send(Message::Text(changed.to_string().into()))
            .await
            .unwrap();
        // A fresh token on the same connection.
        let again = text_of(&mut socket).await;
        assert_eq!(again["token"], "t2");
        let ready = json!({ "type": "ready", "expires": 1790003600u64, "heartbeat": 25 });
        socket
            .send(Message::Text(ready.to_string().into()))
            .await
            .unwrap();
        let notice = json!({ "type": "notice", "kind": "fileRequest", "id": "r1" });
        socket
            .send(Message::Text(notice.to_string().into()))
            .await
            .unwrap();
        socket
            .close(Some(CloseFrame {
                code: CloseCode::from(4401),
                reason: "expired".into(),
            }))
            .await
            .unwrap();
        path
    });

    let mut channel =
        Channel::Realtime(Realtime::connect(&server, "t1", Some("c9")).await.unwrap());
    assert_eq!(channel.expires(), Some(1_790_000_000));
    assert_eq!(
        channel.next().await.unwrap(),
        Event::Changed {
            areas: vec!["vault".into(), "uwu".into()]
        }
    );
    channel.reauth("t2", Some("c10")).await.unwrap();
    assert_eq!(
        channel.next().await.unwrap(),
        Event::Notice {
            kind: "fileRequest".into(),
            id: Some("r1".into())
        }
    );
    assert_eq!(channel.expires(), Some(1_790_003_600));
    let closed = channel.next().await.unwrap_err();
    assert!(closed.needs_token(), "{closed}");
    assert_eq!(fake.await.unwrap(), "/uwu/v1/realtime");
}

#[tokio::test]
async fn a_realtime_server_that_refuses_is_a_close_not_a_hang() {
    let (listener, server) = listen().await;
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket =
            tokio_tungstenite::accept_hdr_async(stream, |_: &Request, mut response: Response| {
                response
                    .headers_mut()
                    .insert("Sec-WebSocket-Protocol", "uwu.realtime.v1".parse().unwrap());
                Ok(response)
            })
            .await
            .unwrap();
        let _ = text_of(&mut socket).await;
        let _ = socket
            .close(Some(CloseFrame {
                code: CloseCode::from(4403),
                reason: "scope".into(),
            }))
            .await;
    });
    let refused = Realtime::connect(&server, "t", None).await.err().unwrap();
    assert_eq!(refused.code, Some(4403));
}

/// `ReceiveMessage` with `{ContextId, Type, Payload: {Id}}`, length-prefixed.
fn hub_frame(kind: u8, context: &str) -> Vec<u8> {
    let mut m = vec![0x95, 0x01, 0x80, 0xc0, 0xa0 | 14];
    m.extend_from_slice(b"ReceiveMessage");
    m.extend_from_slice(&[0x91, 0x83, 0xa0 | 9]);
    m.extend_from_slice(b"ContextId");
    m.push(0xa0 | context.len() as u8);
    m.extend_from_slice(context.as_bytes());
    m.push(0xa0 | 4);
    m.extend_from_slice(b"Type");
    m.push(kind);
    m.push(0xa0 | 7);
    m.extend_from_slice(b"Payload");
    m.extend_from_slice(&[0x81, 0xa0 | 2]);
    m.extend_from_slice(b"Id");
    m.extend_from_slice(&[0xa0 | 2]);
    m.extend_from_slice(b"x1");
    let mut frame = vec![m.len() as u8];
    frame.extend_from_slice(&m);
    frame
}

#[tokio::test]
async fn the_hub_says_sync_and_log_out_but_not_what_this_device_did() {
    let (listener, server) = listen().await;
    let fake = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut query = String::new();
        let mut socket =
            tokio_tungstenite::accept_hdr_async(stream, |request: &Request, response: Response| {
                query = format!(
                    "{}?{}",
                    request.uri().path(),
                    request.uri().query().unwrap_or("")
                );
                Ok(response)
            })
            .await
            .unwrap();
        let Message::Text(handshake) = socket.next().await.unwrap().unwrap() else {
            panic!("no handshake");
        };
        assert_eq!(
            handshake.as_str(),
            "{\"protocol\":\"messagepack\",\"version\":1}\u{1e}"
        );
        socket.send(Message::Text("{}\u{1e}".into())).await.unwrap();
        // This device's own change, somebody else's, then the end of the session.
        socket
            .send(Message::Binary(hub_frame(0, "this-device").into()))
            .await
            .unwrap();
        socket
            .send(Message::Binary(hub_frame(1, "other-device").into()))
            .await
            .unwrap();
        socket
            .send(Message::Binary(hub_frame(11, "other-device").into()))
            .await
            .unwrap();
        // Wait for the client to go.
        while let Some(Ok(_)) = socket.next().await {}
        query
    });

    let mut channel = Channel::Hub(Hub::connect(&server, "tok", "this-device").await.unwrap());
    assert_eq!(channel.expires(), None);
    assert_eq!(
        channel.next().await.unwrap(),
        Event::Changed {
            areas: vec!["vault".into()]
        }
    );
    assert_eq!(
        channel.next().await.unwrap(),
        Event::LogOut {
            reason: "logOut".into()
        }
    );
    channel.close().await;
    assert_eq!(fake.await.unwrap(), "/notifications/hub?access_token=tok");
}
