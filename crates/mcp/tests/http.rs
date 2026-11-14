//! The streamable HTTP transport: a POST is one JSON-RPC exchange, the key
//! travels as a Bearer header, and the router can be mounted without its own
//! auth (that is how the nsh-mail server will reuse it).

mod support;

use std::sync::Arc;
use std::time::Duration;

use notify_mcp::api::NotifyApi;
use notify_mcp::http::client_router;
use serde_json::{Value, json};

async fn start_standalone(stub: String, key: Option<String>) -> String {
    let router = client_router(stub, key, Duration::from_secs(5));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = notify_mcp::http::serve(listener, router).await;
    });
    format!("http://{addr}/mcp")
}

#[tokio::test]
async fn http_transport_answers_the_handshake_and_tool_calls() {
    let stub = support::serve().await;
    let url = start_standalone(stub, Some(support::KEY.to_string())).await;
    let client = reqwest::Client::new();

    let init: Value = post(
        &client,
        &url,
        Some(support::KEY),
        json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {"protocolVersion": "2025-06-18", "capabilities": {},
                       "clientInfo": {"name": "http-test", "version": "0"}}
        }),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(init["result"]["serverInfo"]["name"], "nsh-notify");

    let list: Value = post(
        &client,
        &url,
        Some(support::KEY),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(list["result"]["tools"].as_array().unwrap().len(), 5);

    let call: Value = post(
        &client,
        &url,
        Some(support::KEY),
        json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
               "params": {"name": "list_sources", "arguments": {}}}),
    )
    .await
    .json()
    .await
    .unwrap();
    let text = call["result"]["content"][0]["text"].as_str().unwrap();
    let sources: Value = serde_json::from_str(text).unwrap();
    assert_eq!(sources["count"], 2);
}

#[tokio::test]
async fn http_transport_refuses_requests_without_its_key() {
    let stub = support::serve().await;
    let url = start_standalone(stub, Some(support::KEY.to_string())).await;
    let client = reqwest::Client::new();

    let response = post(
        &client,
        &url,
        None,
        json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}),
    )
    .await;
    assert_eq!(response.status(), 401);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["code"], "ERR_AUTH");

    let response = post(
        &client,
        &url,
        Some("nb_wrong"),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    )
    .await;
    assert_eq!(response.status(), 401);

    // A notification is accepted without an answer body.
    let response = post(
        &client,
        &url,
        Some(support::KEY),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
    )
    .await;
    assert_eq!(response.status(), 202);

    let get = client.get(&url).send().await.unwrap();
    assert_eq!(get.status(), 405);
}

/// What the server does later: mount the router and do its own API-key check.
#[tokio::test]
async fn mounted_router_trusts_its_host_and_its_backend() {
    let stub = support::serve().await;
    let make = Arc::new(move |_key: Option<&str>| {
        Arc::new(NotifyApi::new(stub.clone(), support::KEY)) as Arc<dyn notify_mcp::Notify>
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = notify_mcp::router(make);
    tokio::spawn(async move {
        let _ = notify_mcp::http::serve(listener, router).await;
    });

    let response = reqwest::Client::new()
        .post(format!("http://{addr}/mcp"))
        .json(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
                      "params": {"name": "get", "arguments": {"id": "msg_1"}}}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.unwrap();
    let message: Value =
        serde_json::from_str(body["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(message["id"], "msg_1");
}

async fn post(
    client: &reqwest::Client,
    url: &str,
    key: Option<&str>,
    body: Value,
) -> reqwest::Response {
    let mut request = client.post(url).json(&body);
    if let Some(key) = key {
        request = request.bearer_auth(key);
    }
    request.send().await.expect("the MCP endpoint answers")
}
