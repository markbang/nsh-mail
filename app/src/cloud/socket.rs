//! The server's event stream as a channel on the engine thread: the
//! browser's WebSocket (NSH's transport) on the web, tokio-tungstenite on
//! the runtime's threads elsewhere. The connection ends when the stream is
//! dropped.

use futures_channel::mpsc::{UnboundedReceiver, unbounded};
use nsh_core::prelude::System;

pub enum Incoming {
    Open,
    Text(String),
}

pub struct Stream {
    rx: UnboundedReceiver<Incoming>,
    #[cfg(not(target_arch = "wasm32"))]
    task: tokio::task::JoinHandle<()>,
    #[cfg(target_arch = "wasm32")]
    _task: Option<Box<dyn nsh_core::websocket::WebSocketTask>>,
}

impl Stream {
    /// The next message; `None` once the connection ended or never opened.
    pub async fn next(&mut self) -> Option<Incoming> {
        use futures_core::Stream as _;
        std::future::poll_fn(|cx| std::pin::Pin::new(&mut self.rx).poll_next(cx)).await
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for Stream {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn connect(url: &str, _sys: &System) -> Stream {
    use futures_util::StreamExt;
    use tokio_tungstenite::tungstenite::Message;

    let (tx, rx) = unbounded();
    let url = url.to_owned();
    let task = nsh_core::prelude::background(async move {
        let mut ws = match tokio_tungstenite::connect_async(url.as_str()).await {
            Ok((ws, _)) => ws,
            Err(e) => return log::info!("no event stream: {e}"),
        };
        if tx.unbounded_send(Incoming::Open).is_err() {
            return;
        }
        // tungstenite answers the server's pings as it reads.
        while let Some(Ok(message)) = ws.next().await {
            if let Message::Text(text) = message
                && tx.unbounded_send(Incoming::Text(text.to_string())).is_err()
            {
                return;
            }
        }
    });
    Stream { rx, task }
}

#[cfg(target_arch = "wasm32")]
pub fn connect(url: &str, sys: &System) -> Stream {
    use nsh_core::websocket::{WebSocketData, WebSocketEvent, WebSocketRequest};

    let (tx, rx) = unbounded();
    let task = sys.websocket.as_ref().and_then(|transport| {
        let request = WebSocketRequest {
            url: url.to_owned(),
            protocols: Vec::new(),
            headers: Vec::new(),
        };
        let events = Box::new(move |event| match event {
            WebSocketEvent::Open { .. } => drop(tx.unbounded_send(Incoming::Open)),
            WebSocketEvent::Message(WebSocketData::Text(text)) => {
                drop(tx.unbounded_send(Incoming::Text(text)))
            }
            WebSocketEvent::Message(WebSocketData::Bytes(_)) => {}
            WebSocketEvent::Error(e) => {
                log::info!("event stream: {e}");
                tx.close_channel();
            }
            WebSocketEvent::Close { .. } => tx.close_channel(),
        });
        transport
            .connect(request, events)
            .inspect_err(|e| log::info!("no event stream: {e}"))
            .ok()
    });
    Stream { rx, _task: task }
}
