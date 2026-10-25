//! The real transport: one `IoTaskPool` task per connection over `async-tungstenite`, and one per
//! fetch (`http.rs`). `wss:` runs over the [`Stream`] `tls.rs` connects.
//!
//! A task connects, reads into a bounded channel the main thread drains on [`Transport::poll`],
//! and writes what an unbounded channel brings it. When the inbound channel is full the task
//! waits, so core sees the overflow and closes with 1009. Dropping the transport closes both
//! channels, which ends every task.

use super::tls::{self, NetTls, Stream};
use super::{ABNORMAL, ConnId, FetchId, Frame, HttpRequest, MAX_MESSAGE_BYTES, MAX_QUEUED_MESSAGES, NetEvent, OpenRequest, TOO_BIG, Transport};
use async_channel::{Receiver, Sender, TryRecvError};
use async_io::Timer;
use async_tungstenite::WebSocketStream;
use async_tungstenite::tungstenite::client::IntoClientRequest;
use async_tungstenite::tungstenite::error::{CapacityError, Error as WsError};
use async_tungstenite::tungstenite::http::{HeaderName, HeaderValue};
use async_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use async_tungstenite::tungstenite::protocol::{CloseFrame, WebSocketConfig};
use async_tungstenite::tungstenite::{Message, Utf8Bytes};
use bevy::tasks::futures_lite::{StreamExt, future};
use bevy::tasks::{IoTaskPool, Task, TaskPool};
use std::collections::HashMap;
use std::time::Duration;

/// How long a closing connection waits for the other side's close before dropping the socket.
const CLOSE_TIMEOUT: Duration = Duration::from_secs(5);

/// Headers the handshake writes itself; a script's `headers` may not repeat them.
const OWN_HEADERS: [&str; 7] = ["host", "connection", "upgrade", "sec-websocket-version", "sec-websocket-key", "sec-websocket-protocol", "origin"];

type Socket = WebSocketStream<Stream>;

/// What the main thread asks of a connection's task.
enum Out {
    Send(Frame),
    Close(Option<u16>, String),
}

struct Conn {
    out: Sender<Out>,
    events: Receiver<NetEvent>,
}

/// Connections and fetches over TCP sockets, with TLS for `wss:` and `https:`.
pub struct SocketTransport {
    tls: NetTls,
    conns: HashMap<ConnId, Conn>,
    /// Fetches in flight: dropping one's task cancels it, closing its connection.
    fetches: HashMap<FetchId, Task<()>>,
    /// Every fetch's one event.
    fetched: (Sender<NetEvent>, Receiver<NetEvent>),
}

impl Default for SocketTransport {
    /// TLS verified by the platform.
    fn default() -> Self {
        Self::new(NetTls::default())
    }
}

impl SocketTransport {
    pub fn new(tls: NetTls) -> Self {
        SocketTransport { tls, conns: HashMap::new(), fetches: HashMap::new(), fetched: async_channel::unbounded() }
    }
}

impl Transport for SocketTransport {
    fn open(&mut self, id: ConnId, request: &OpenRequest) {
        let (out, out_rx) = async_channel::unbounded();
        let (events_tx, events) = async_channel::bounded(MAX_QUEUED_MESSAGES + 1);
        let request = request.clone();
        let tls = self.tls.clone();
        IoTaskPool::get_or_init(TaskPool::new).spawn(run(id, request, tls, events_tx, out_rx)).detach();
        self.conns.insert(id, Conn { out, events });
    }

    fn fetch(&mut self, id: FetchId, request: &HttpRequest) {
        let events = self.fetched.0.clone();
        let task = IoTaskPool::get_or_init(TaskPool::new).spawn(super::http::exchange(id, request.clone(), self.tls.clone(), events));
        self.fetches.insert(id, task);
    }

    fn cancel(&mut self, id: FetchId) {
        self.fetches.remove(&id);
    }

    fn send(&mut self, id: ConnId, frame: Frame) {
        if let Some(c) = self.conns.get(&id) {
            let _ = c.out.try_send(Out::Send(frame));
        }
    }

    fn close(&mut self, id: ConnId, code: Option<u16>, reason: &str) {
        if let Some(c) = self.conns.get(&id) {
            let _ = c.out.try_send(Out::Close(code, reason.to_string()));
        }
    }

    fn poll(&mut self, out: &mut Vec<NetEvent>) {
        self.conns.retain(|_, c| loop {
            match c.events.try_recv() {
                Ok(event) => out.push(event),
                Err(TryRecvError::Empty) => break true,
                Err(TryRecvError::Closed) => break false,
            }
        });
        while let Ok(event) = self.fetched.1.try_recv() {
            if let NetEvent::Response { id, .. } | NetEvent::FetchFailed { id, .. } = &event {
                self.fetches.remove(id);
            }
            out.push(event);
        }
    }
}

fn close_event(id: ConnId, code: u16, reason: String, clean: bool) -> NetEvent {
    NetEvent::Close { id, code, reason, clean }
}

/// One connection, from DNS to its last event.
async fn run(id: ConnId, request: OpenRequest, tls: NetTls, events: Sender<NetEvent>, out: Receiver<Out>) {
    // Connecting: a close abandons it with 1006 (core drops it if it forgot the id); a dropped
    // transport abandons it with nothing. A failure's reason rides on the close for core to log.
    let connected = future::or(async { Some(connect(&request, &tls).await) }, async {
        while let Ok(Out::Send(_)) = out.recv().await {}
        None
    })
    .await;
    let mut ws = match connected {
        None => {
            if !out.is_closed() {
                let _ = events.send(close_event(id, ABNORMAL, String::new(), false)).await;
            }
            return;
        }
        Some(Err(why)) => {
            let _ = events.send(close_event(id, ABNORMAL, why, false)).await;
            return;
        }
        Some(Ok((ws, protocol))) => {
            if events.send(NetEvent::Open { id, protocol, extensions: String::new() }).await.is_err() {
                return;
            }
            ws
        }
    };

    enum Step {
        In(Option<Result<Message, WsError>>),
        Out(Option<Out>),
    }
    loop {
        let step = future::or(async { Step::In(ws.next().await) }, async { Step::Out(out.recv().await.ok()) }).await;
        let event = match step {
            Step::Out(None) => return,
            Step::Out(Some(Out::Send(frame))) => {
                let bytes = frame.len();
                let message = match frame {
                    Frame::Text(s) => Message::text(s),
                    Frame::Binary(b) => Message::binary(b),
                };
                match ws.send(message).await {
                    Ok(()) => NetEvent::Written { id, bytes },
                    Err(_) => close_event(id, ABNORMAL, String::new(), false),
                }
            }
            Step::Out(Some(Out::Close(code, reason))) => {
                let frame = code.map(|code| CloseFrame { code: CloseCode::from(code), reason: Utf8Bytes::from(reason) });
                let event = match ws.close(frame).await {
                    Ok(()) => closing(id, &mut ws).await,
                    Err(_) => close_event(id, ABNORMAL, String::new(), false),
                };
                let _ = events.send(event).await;
                return;
            }
            Step::In(Some(Ok(Message::Text(t)))) => NetEvent::Message { id, frame: Frame::Text(t.as_str().to_string()) },
            Step::In(Some(Ok(Message::Binary(b)))) => NetEvent::Message { id, frame: Frame::Binary(b.to_vec()) },
            Step::In(Some(Ok(Message::Close(frame)))) => {
                let _ = events.send(peer_close(id, frame)).await;
                // Flush the reply tungstenite queued, then let the server end the TCP stream.
                let _ = future::or(async { while ws.next().await.is_some() {} }, async {
                    Timer::after(CLOSE_TIMEOUT).await;
                })
                .await;
                return;
            }
            Step::In(Some(Ok(_))) => continue,
            Step::In(Some(Err(WsError::Capacity(CapacityError::MessageTooLong { .. })))) => {
                let frame = CloseFrame { code: CloseCode::from(TOO_BIG), reason: Utf8Bytes::from_static("message too big") };
                let _ = events.send(close_event(id, TOO_BIG, "message too big".into(), false)).await;
                let _ = future::or(async { ws.close(Some(frame)).await.ok() }, async {
                    Timer::after(CLOSE_TIMEOUT).await;
                    None
                })
                .await;
                return;
            }
            Step::In(Some(Err(_)) | None) => {
                let _ = events.send(close_event(id, ABNORMAL, String::new(), false)).await;
                return;
            }
        };
        let last = matches!(event, NetEvent::Close { .. });
        if events.send(event).await.is_err() || last {
            return;
        }
    }
}

/// After our close frame: the server's close, or 1006 when it drops or does not answer in time.
async fn closing(id: ConnId, ws: &mut Socket) -> NetEvent {
    let wait = async {
        while let Some(r) = ws.next().await {
            match r {
                Ok(Message::Close(frame)) => return peer_close(id, frame),
                Ok(_) => {}
                Err(_) => break,
            }
        }
        close_event(id, ABNORMAL, String::new(), false)
    };
    future::or(wait, async {
        Timer::after(CLOSE_TIMEOUT).await;
        close_event(id, ABNORMAL, String::new(), false)
    })
    .await
}

/// A close frame the server sent: its code and reason, or 1005 when it carried none.
fn peer_close(id: ConnId, frame: Option<CloseFrame>) -> NetEvent {
    match frame {
        Some(f) => close_event(id, u16::from(f.code), f.reason.as_str().to_string(), true),
        None => close_event(id, 1005, String::new(), true),
    }
}

/// Resolves, connects, handshakes TLS for `wss:`, then the WebSocket; returns the socket and the
/// protocol the server chose, or the reason it failed.
async fn connect(request: &OpenRequest, tls: &NetTls) -> Result<(Socket, String), String> {
    let bad = |e: &dyn std::fmt::Display| format!("a request the engine cannot send: {e}");
    let mut req = request.url.as_str().into_client_request().map_err(|e| bad(&e))?;
    let secure = match req.uri().scheme_str() {
        Some("ws") => false,
        Some("wss") => true,
        _ => return Err("not a ws: or wss: URL".into()),
    };
    let host = req.uri().host().ok_or("no host")?.to_string();
    let port = req.uri().port_u16().unwrap_or(if secure { 443 } else { 80 });
    let headers = req.headers_mut();
    if !request.protocols.is_empty() {
        headers.insert("sec-websocket-protocol", HeaderValue::from_str(&request.protocols.join(", ")).map_err(|e| bad(&e))?);
    }
    for (name, value) in &request.headers {
        if OWN_HEADERS.iter().any(|h| h.eq_ignore_ascii_case(name)) {
            continue;
        }
        let name = HeaderName::from_bytes(name.as_bytes()).map_err(|e| bad(&e))?;
        headers.append(name, HeaderValue::from_str(value).map_err(|e| bad(&e))?);
    }

    let stream = tls::connect(&host, port, secure.then_some(tls)).await?;
    let config = WebSocketConfig::default().max_message_size(Some(MAX_MESSAGE_BYTES)).max_frame_size(Some(MAX_MESSAGE_BYTES));
    let (ws, response) = async_tungstenite::client_async_with_config(req, stream, Some(config)).await.map_err(|e| format!("the WebSocket handshake failed: {e}"))?;
    let protocol = response.headers().get("sec-websocket-protocol").and_then(|v| v.to_str().ok()).unwrap_or_default().to_string();
    Ok((ws, protocol))
}
