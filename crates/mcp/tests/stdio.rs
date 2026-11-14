//! A real stdio session: the JSON-RPC handshake, the tool list, two tool calls
//! and a failing call, driven through the built binary.

mod support;

use std::process::Stdio;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::Command;

#[tokio::test]
async fn stdio_session_speaks_mcp() {
    let base = support::serve().await;
    let mut child = Command::new(env!("CARGO_BIN_EXE_nsh-notify-mcp"))
        .args(["--server", &base, "--key", support::KEY])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the binary starts");
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap()).lines();

    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "stdio-test", "version": "0"}
            }
        }),
    )
    .await;
    let init = next(&mut stdout).await;
    assert_eq!(init["id"], 1);
    assert_eq!(init["result"]["serverInfo"]["name"], "nsh-notify");
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert!(init["result"]["capabilities"]["tools"].is_object());

    // A notification owes no answer, so the next line is tools/list's.
    send(
        &mut stdin,
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
    )
    .await;
    send(
        &mut stdin,
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    )
    .await;
    let list = next(&mut stdout).await;
    assert_eq!(list["id"], 2);
    let tools: Vec<String> = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        tools,
        ["list_unread", "search", "get", "mark_read", "list_sources"]
    );
    for tool in list["result"]["tools"].as_array().unwrap() {
        assert!(tool["description"].as_str().is_some_and(|d| d.len() > 40));
        assert_eq!(tool["inputSchema"]["type"], "object");
    }

    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": {"name": "list_unread", "arguments": {"limit": 2}}
        }),
    )
    .await;
    let call = next(&mut stdout).await;
    assert_eq!(call["id"], 3);
    assert!(call["result"]["isError"].is_null());
    let page: Value =
        serde_json::from_str(call["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(page["count"], 2);
    assert_eq!(page["messages"][0]["id"], "msg_1");
    assert!(page["next"].is_string());

    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0", "id": 4, "method": "tools/call",
            "params": {"name": "get", "arguments": {"id": "missing"}}
        }),
    )
    .await;
    let failed = next(&mut stdout).await;
    assert_eq!(failed["id"], 4);
    assert_eq!(failed["result"]["isError"], true);
    let text = failed["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("no such message: missing"), "{text}");

    child.kill().await.expect("the process stops");
}

async fn send(stdin: &mut tokio::process::ChildStdin, message: Value) {
    let mut line = message.to_string();
    line.push('\n');
    stdin.write_all(line.as_bytes()).await.unwrap();
    stdin.flush().await.unwrap();
}

async fn next(stdout: &mut Lines<BufReader<tokio::process::ChildStdout>>) -> Value {
    let line = tokio::time::timeout(Duration::from_secs(10), stdout.next_line())
        .await
        .expect("an answer within 10s")
        .expect("stdout stays open")
        .expect("a line of JSON");
    serde_json::from_str(&line).unwrap_or_else(|e| panic!("not JSON-RPC: {e}: {line}"))
}
