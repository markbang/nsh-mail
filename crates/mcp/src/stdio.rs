//! stdio transport: one JSON-RPC message per line on stdin, answers on stdout.
//! Nothing else may write to stdout — logs go to stderr.

use std::io;
use std::sync::Arc;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};

use crate::api::Notify;
use crate::protocol;

pub async fn serve(backend: Arc<dyn Notify>) -> io::Result<()> {
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut out = BufWriter::new(tokio::io::stdout());

    while let Some(line) = lines.next_line().await? {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(e) => {
                let error = json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": { "code": -32700, "message": format!("invalid JSON: {e}") },
                });
                write(&mut out, &error).await?;
                continue;
            }
        };

        let answer = match request {
            Value::Array(items) => {
                let mut answers = Vec::with_capacity(items.len());
                for item in &items {
                    if let Some(answer) = protocol::dispatch(backend.as_ref(), item).await {
                        answers.push(answer);
                    }
                }
                match answers.len() {
                    0 => continue,
                    1 => answers.remove(0),
                    _ => Value::Array(answers),
                }
            }
            one => match protocol::dispatch(backend.as_ref(), &one).await {
                Some(answer) => answer,
                None => continue,
            },
        };
        write(&mut out, &answer).await?;
    }
    Ok(())
}

async fn write(out: &mut BufWriter<tokio::io::Stdout>, value: &Value) -> io::Result<()> {
    let mut text = serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string());
    text.push('\n');
    out.write_all(text.as_bytes()).await?;
    out.flush().await
}
