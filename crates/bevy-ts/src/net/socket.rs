//! The real transport: one `IoTaskPool` task per connection over `async-tungstenite`, with
//! `wss:` over rustls (ring, webpki roots).
//!
//! A task connects, reads into a bounded channel the main thread drains on [`Transport::poll`],
//! and writes what an unbounded channel brings it. When the inbound channel is full the task
//! waits, so core sees the overflow and closes with 1009. Dropping the transport closes both
//! channels, which ends every task.

use super::{ABNORMAL, ConnId, Frame, MAX_MESSAGE_BYTES, MAX_QUEUED_MESSAGES, NetEvent, OpenRequest, TOO_BIG, Transport};
use async_channel::{Receiver, Sender, TryRecvError};
use async_io::{Async, Timer};
use async_tungstenite::WebSocketStream;
use async_tungstenite::tungstenite::client::IntoClientRequest;
use async_tungstenite::tungstenite::error::{CapacityError, Error as WsError};
use async_tungstenite::tungstenite::http::{HeaderName, HeaderValue};
use async_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use async_tungstenite::tungstenite::protocol::{CloseFrame, WebSocketConfig};
use async_tungstenite::tungstenite::{Message, Utf8Bytes};
use bevy::tasks::futures_lite::io::{AsyncRead, AsyncWrite};
use bevy::tasks::futures_lite::{StreamExt, future};
use bevy::tasks::{IoTaskPool, TaskPool};
use futures_rustls::TlsConnector;
use futures_rustls::client::TlsStream;
use futures_rustls::rustls::pki_types::{CertificateDer, ServerName};
use futures_rustls::rustls::{ClientConfig, RootCertStore, crypto};
use std::collections::HashMap;
use std::io;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

/// How long a closing connection waits for the other side's close before dropping the socket.
const CLOSE_TIMEOUT: Duration = Duration::from_secs(5);

/// Headers the handshake writes itself; a script's `headers` may not repeat them.
const OWN_HEADERS: [&str; 7] = ["host", "connection", "upgrade", "sec-websocket-version", "sec-websocket-key", "sec-websocket-protocol", "origin"];

type Socket = WebSocketStream<Stream>;

/// A connection's bytes: plain for `ws:`, TLS for `wss:`.
enum Stream {
    Plain(Async<TcpStream>),
    Tls(Box<TlsStream<Async<TcpStream>>>),
}

impl AsyncRead for Stream {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Stream::Plain(s) => Pin::new(s).poll_read(cx, buf),
            Stream::Tls(s) => Pin::new(s.as_mut()).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for Stream {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Stream::Plain(s) => Pin::new(s).poll_write(cx, buf),
            Stream::Tls(s) => Pin::new(s.as_mut()).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Stream::Plain(s) => Pin::new(s).poll_flush(cx),
            Stream::Tls(s) => Pin::new(s.as_mut()).poll_flush(cx),
        }
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Stream::Plain(s) => Pin::new(s).poll_close(cx),
            Stream::Tls(s) => Pin::new(s.as_mut()).poll_close(cx),
        }
    }
}

/// What the main thread asks of a connection's task.
enum Out {
    Send(Frame),
    Close(Option<u16>, String),
}

struct Conn {
    out: Sender<Out>,
    events: Receiver<NetEvent>,
}

/// What a connection's task needs besides its request.
#[derive(Clone)]
struct Dial {
    tls: Arc<ClientConfig>,
    /// Hosts that connect to an address without DNS.
    resolve: Arc<HashMap<String, SocketAddr>>,
    log: Sender<String>,
}

/// Connections over TCP sockets: `ws:`, and `wss:` verified against the webpki roots and the
/// URL's host name.
pub struct SocketTransport {
    conns: HashMap<ConnId, Conn>,
    /// Roots trusted besides webpki's: tests' only.
    roots: Vec<CertificateDer<'static>>,
    resolve: HashMap<String, SocketAddr>,
    /// Built at the first open, from the roots.
    dial: Option<Dial>,
    log_tx: Sender<String>,
    log: Receiver<String>,
}

impl Default for SocketTransport {
    fn default() -> Self {
        let (log_tx, log) = async_channel::unbounded();
        SocketTransport { conns: HashMap::new(), roots: Vec::new(), resolve: HashMap::new(), dial: None, log_tx, log }
    }
}

impl SocketTransport {
    /// Trusts `der`, a DER certificate, as a root besides webpki's. For tests, from Rust only:
    /// nothing a script or a project file names reaches it.
    pub fn with_root(mut self, der: impl Into<Vec<u8>>) -> Self {
        self.roots.push(CertificateDer::from(der.into()));
        self.dial = None;
        self
    }

    /// Connects `host` to `addr` without a DNS lookup, as curl's `--resolve` does; the TLS name
    /// and the `Host` header stay `host`. For tests, from Rust only.
    pub fn with_resolve(mut self, host: &str, addr: SocketAddr) -> Self {
        self.resolve.insert(host.to_ascii_lowercase(), addr);
        self.dial = None;
        self
    }

    fn dial(&mut self) -> Dial {
        if let Some(d) = &self.dial {
            return d.clone();
        }
        let mut roots = RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
        for der in &self.roots {
            if let Err(e) = roots.add(der.clone()) {
                bevy::log::warn!("a test root is skipped: {e}");
            }
        }
        let tls = ClientConfig::builder_with_provider(Arc::new(crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .expect("ring supports the default protocol versions")
            .with_root_certificates(roots)
            .with_no_client_auth();
        let d = Dial { tls: Arc::new(tls), resolve: Arc::new(self.resolve.clone()), log: self.log_tx.clone() };
        self.dial = Some(d.clone());
        d
    }
}

impl Transport for SocketTransport {
    fn open(&mut self, id: ConnId, request: &OpenRequest) {
        let (out, out_rx) = async_channel::unbounded();
        let (events_tx, events) = async_channel::bounded(MAX_QUEUED_MESSAGES + 1);
        let request = request.clone();
        let dial = self.dial();
        IoTaskPool::get_or_init(TaskPool::new).spawn(run(id, request, dial, events_tx, out_rx)).detach();
        self.conns.insert(id, Conn { out, events });
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
    }

    fn take_log(&mut self, out: &mut Vec<String>) {
        while let Ok(line) = self.log.try_recv() {
            out.push(line);
        }
    }
}

fn close_event(id: ConnId, code: u16, reason: String, clean: bool) -> NetEvent {
    NetEvent::Close { id, code, reason, clean }
}

/// One connection, from DNS to its last event.
async fn run(id: ConnId, request: OpenRequest, dial: Dial, events: Sender<NetEvent>, out: Receiver<Out>) {
    // Connecting: a close abandons it with 1006 (core drops it if it forgot the id); a dropped
    // transport abandons it with nothing.
    let connected = future::or(async { Some(connect(&request, &dial).await) }, async {
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
            if let Some(line) = why {
                // Before the close, so the registry has the line when it takes the close.
                let _ = dial.log.send(line).await;
            }
            let _ = events.send(close_event(id, ABNORMAL, String::new(), false)).await;
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

/// Resolves, connects, then TLS for `wss:`, and handshakes; returns the socket and the protocol
/// the server chose. A failure worth a log line (TLS) carries it.
async fn connect(request: &OpenRequest, dial: &Dial) -> Result<(Socket, String), Option<String>> {
    // `http:` reads as `ws:` and `https:` as `wss:`.
    let url = match request.url.split_once("://") {
        Some((scheme, rest)) if scheme.eq_ignore_ascii_case("http") => format!("ws://{rest}"),
        Some((scheme, rest)) if scheme.eq_ignore_ascii_case("https") => format!("wss://{rest}"),
        _ => request.url.clone(),
    };
    let mut req = url.as_str().into_client_request().map_err(|_| None)?;
    let secure = match req.uri().scheme_str() {
        Some("ws") => false,
        Some("wss") => true,
        _ => return Err(None),
    };
    let host = req.uri().host().ok_or(None)?.trim_start_matches('[').trim_end_matches(']').to_ascii_lowercase();
    let port = req.uri().port_u16().unwrap_or(if secure { 443 } else { 80 });
    let headers = req.headers_mut();
    if !request.protocols.is_empty() {
        headers.insert("sec-websocket-protocol", HeaderValue::from_str(&request.protocols.join(", ")).map_err(|_| None)?);
    }
    for (name, value) in &request.headers {
        if OWN_HEADERS.iter().any(|h| h.eq_ignore_ascii_case(name)) {
            continue;
        }
        let name = HeaderName::from_bytes(name.as_bytes()).map_err(|_| None)?;
        headers.append(name, HeaderValue::from_str(value).map_err(|_| None)?);
    }

    let addrs: Vec<SocketAddr> = match dial.resolve.get(&host) {
        Some(addr) => vec![*addr],
        None => {
            let h = host.clone();
            blocking::unblock(move || (h.as_str(), port).to_socket_addrs().map(Iterator::collect)).await.map_err(|_| None)?
        }
    };
    let mut stream = None;
    for addr in addrs {
        if let Ok(s) = Async::<TcpStream>::connect(addr).await {
            stream = Some(s);
            break;
        }
    }
    let tcp = stream.ok_or(None)?;
    let stream = if secure {
        let name = ServerName::try_from(host).map_err(|_| None)?;
        let tls = TlsConnector::from(dial.tls.clone()).connect(name, tcp).await.map_err(|e| Some(format!("WebSocket {}: TLS failed: {e}", request.url)))?;
        Stream::Tls(Box::new(tls))
    } else {
        Stream::Plain(tcp)
    };

    let config = WebSocketConfig::default().max_message_size(Some(MAX_MESSAGE_BYTES)).max_frame_size(Some(MAX_MESSAGE_BYTES));
    let (ws, response) = async_tungstenite::client_async_with_config(req, stream, Some(config)).await.map_err(|_| None)?;
    let protocol = response.headers().get("sec-websocket-protocol").and_then(|v| v.to_str().ok()).unwrap_or_default().to_string();
    Ok((ws, protocol))
}
