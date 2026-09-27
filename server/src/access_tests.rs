use tokio::io::{
    AsyncReadExt,
    AsyncWriteExt,
};

use super::*;

async fn start() -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let (events, _) = broadcast::channel(64);
    let state = AppState {
        ws: Arc::new(Mutex::new(Workspace::new(
            Arc::new(DemoProvider),
            None,
            None,
            true,
        ))),
        events,
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let access = access::Access::new("a".repeat(64), addr, vec!["http://localhost:5173".into()]);
    let server = tokio::spawn(async move {
        axum::serve(listener, api_router(state, access))
            .await
            .unwrap()
    });
    (addr, server)
}

async fn request(path: &str, headers: &str) -> String {
    let (addr, server) = start().await;
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let request = if path == "/ws" {
        format!("GET /ws HTTP/1.1\r\nHost: {addr}\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n{headers}\r\n")
    } else {
        let body = r#"{"method":"state"}"#;
        format!("POST {path} HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{headers}\r\n{body}", body.len())
    };
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut data = [0; 4096];
    let count = tokio::time::timeout(Duration::from_secs(3), stream.read(&mut data))
        .await
        .unwrap()
        .unwrap();
    server.abort();
    String::from_utf8_lossy(&data[..count]).into_owned()
}

#[tokio::test]
async fn all_endpoints_require_token_even_with_trusted_or_missing_origin() {
    for path in ["/ws", "/api/rpc", "/api/auth"] {
        for headers in [
            "",
            "Origin: http://localhost:5173\r\n",
            "Authorization: Bearer wrong\r\n",
            "Cookie: token=ignored\r\n",
            "Sec-WebSocket-Protocol: reclass, reclass-token.wrong\r\n",
        ] {
            assert!(request(path, headers).await.starts_with("HTTP/1.1 401"));
        }
    }
    assert!(request("/api/rpc?token=ignored", "")
        .await
        .starts_with("HTTP/1.1 401"));
}

#[tokio::test]
async fn valid_token_does_not_bypass_origin_validation() {
    for path in ["/ws", "/api/rpc", "/api/auth"] {
        for origin in [
            "https://untrusted.example",
            "null",
            "http://localhost:5174",
            "http://localhost:5173.evil.example",
            "http://localhost:5173/",
            "http://localhost:5173, https://untrusted.example",
        ] {
            let headers = format!("Authorization: Bearer {}\r\nOrigin: {origin}\r\nX-Forwarded-Host: localhost:5173\r\n", "a".repeat(64));
            assert!(request(path, &headers).await.starts_with("HTTP/1.1 403"));
        }
        for headers in [
            format!("Authorization: Bearer {}\r\nOrigin: http://localhost:5173\r\nOrigin: https://untrusted.example\r\n", "a".repeat(64)),
            format!("Authorization: Bearer {}\r\nAuthorization: Bearer wrong\r\n", "a".repeat(64)),
        ] {
            assert!(request(path, &headers).await.starts_with("HTTP/1.1 400"));
        }
    }
}

#[tokio::test]
async fn auth_probe_accepts_valid_token_without_workspace_access() {
    let headers = format!(
        "Authorization: Bearer {}\r\nOrigin: http://localhost:5173\r\n",
        "a".repeat(64)
    );
    assert!(request("/api/auth", &headers)
        .await
        .starts_with("HTTP/1.1 204"));
}

#[tokio::test]
async fn authenticated_rpc_accepts_trusted_browser_and_native_clients() {
    for origin in ["", "Origin: http://localhost:5173\r\n"] {
        let headers = format!("Authorization: Bearer {}\r\n{origin}", "a".repeat(64));
        let response = request("/api/rpc", &headers).await;
        assert!(response.starts_with("HTTP/1.1 200"));
    }
    let headers = format!(
        "Sec-WebSocket-Protocol: reclass, reclass-token.{}\r\n",
        "a".repeat(64)
    );
    assert!(request("/api/rpc", &headers)
        .await
        .starts_with("HTTP/1.1 401"));
}

#[tokio::test]
async fn authenticated_websocket_receives_session_and_executes_commands() {
    use tokio_tungstenite::tungstenite::{
        client::IntoClientRequest,
        Message,
    };
    for browser in [false, true] {
        let (addr, server) = start().await;
        let mut req = format!("ws://{addr}/ws").into_client_request().unwrap();
        if browser {
            req.headers_mut()
                .insert("Origin", format!("http://{addr}").parse().unwrap());
            req.headers_mut().insert(
                "Sec-WebSocket-Protocol",
                format!("reclass, reclass-token.{}", "a".repeat(64))
                    .parse()
                    .unwrap(),
            );
        } else {
            req.headers_mut().insert(
                "Authorization",
                format!("Bearer {}", "a".repeat(64)).parse().unwrap(),
            );
        }
        let (mut socket, response) = tokio_tungstenite::connect_async(req).await.unwrap();
        if browser {
            assert_eq!(response.headers()["sec-websocket-protocol"], "reclass");
        }
        for expected in ["session", "defs", "frame"] {
            let message = tokio::time::timeout(Duration::from_secs(3), socket.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            let value: Value = serde_json::from_str(message.to_text().unwrap()).unwrap();
            assert_eq!(value["type"], expected);
        }
        socket
            .send(Message::Text(r#"{"id":7,"method":"state"}"#.into()))
            .await
            .unwrap();
        let reply = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let message = socket.next().await.unwrap().unwrap();
                let value: Value = serde_json::from_str(message.to_text().unwrap()).unwrap();
                if value["type"] == "reply" {
                    break value;
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(reply["id"], 7);
        assert_eq!(reply["ok"], true);
        server.abort();
    }
}

#[test]
fn origin_configuration_rejects_patterns_and_urls_with_paths() {
    for origin in [
        "*",
        "null",
        "https://*.example.com",
        "https://example.com/",
        "https://example.com/path",
        "https://user@example.com",
        "https://example.com?query",
        "https://example.com#fragment",
    ] {
        assert!(access::parse_origin(origin).is_err(), "accepted {origin}");
    }
    for origin in [
        "http://localhost:5173",
        "https://example.com",
        "http://[::1]:7878",
    ] {
        assert!(access::parse_origin(origin).is_ok(), "rejected {origin}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pipelined_websocket_commands_execute_in_arrival_order() {
    use tokio_tungstenite::tungstenite::{
        client::IntoClientRequest,
        Message,
    };

    let (addr, server) = start().await;
    let mut req = format!("ws://{addr}/ws").into_client_request().unwrap();
    req.headers_mut().insert(
        "Authorization",
        format!("Bearer {}", "a".repeat(64)).parse().unwrap(),
    );
    let (mut socket, _) = tokio_tungstenite::connect_async(req).await.unwrap();
    let result = tokio::time::timeout(Duration::from_secs(10), async {
        let mut defs = Value::Null;
        for _ in 0..3 {
            let message = socket.next().await.unwrap().unwrap();
            let value: Value = serde_json::from_str(message.to_text().unwrap()).unwrap();
            if value["type"] == "defs" {
                defs = value;
            }
        }
        let class = &defs["classes"][0];
        let class_id = class["id"].as_u64().unwrap();
        let field_id = class["fields"][0]["id"].as_u64().unwrap();
        // Queue dependent edits without waiting for replies. Resetting the field
        // between pairs also catches commands overtaking a previous pair.
        let mut id = 0;
        for _ in 0..100 {
            for (method, params) in [
                ("retype", json!({ "classId": class_id, "fieldId": field_id, "ty": "Hex64" })),
                ("retype", json!({ "classId": class_id, "fieldId": field_id, "ty": "Pointer" })),
                ("setPointerTarget", json!({ "classId": class_id, "fieldId": field_id, "target": { "FieldType": "Int32" } })),
            ] {
                socket.feed(Message::Text(json!({ "id": id, "method": method, "params": params }).to_string().into())).await.unwrap();
                id += 1;
            }
        }
        socket.flush().await.unwrap();
        for expected in 0..id {
            loop {
                let message = socket.next().await.unwrap().unwrap();
                let value: Value = serde_json::from_str(message.to_text().unwrap()).unwrap();
                if value["type"] == "reply" {
                    assert_eq!(value["ok"], true, "command failed: {value}");
                    assert_eq!(value["id"], expected, "reply arrived out of order");
                    break;
                }
            }
        }
    }).await;
    server.abort();
    result.unwrap();
}

#[tokio::test]
async fn rejects_untrusted_websocket_origin() {
    let response = request("/ws", "Origin: https://untrusted.example\r\n").await;
    assert!(
        response.starts_with("HTTP/1.1 403"),
        "unexpected response status: {}",
        response.lines().next().unwrap()
    );
}

#[tokio::test]
async fn rejects_unauthenticated_rpc() {
    let response = request("/api/rpc", "").await;
    assert!(
        response.starts_with("HTTP/1.1 401"),
        "unexpected response status: {}",
        response.lines().next().unwrap()
    );
}
