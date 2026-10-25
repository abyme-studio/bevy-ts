//! `SocketTransport` against a local sync `tungstenite` server on an ephemeral port, in clear and,
//! for `wss:`, over rustls with the test authority in `fixtures/tls` (README.md there).

use bevy::tasks::{IoTaskPool, TaskPool};
use bevy_ts::net::{ConnId, Frame, NetEvent, NetTls, OpenRequest, SocketTransport, Transport};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};
use tungstenite::handshake::server::{Request, Response};
use tungstenite::http::HeaderValue;
use tungstenite::protocol::CloseFrame;
use tungstenite::protocol::frame::coding::CloseCode;
use tungstenite::{Message, WebSocket};

const ID: ConnId = ConnId(1);

/// What the server saw of the handshake request.
struct Seen {
    headers: Vec<(String, String)>,
}

/// Serves one connection on a thread: picks `protocol` if offered, records the request, then
/// runs `serve`. Returns the `ws:` URL and the request it saw.
// tungstenite's `accept_hdr` callback returns its own large `ErrorResponse`.
#[allow(clippy::result_large_err)]
fn server(protocol: Option<&'static str>, serve: impl FnOnce(&mut WebSocket<TcpStream>) + Send + 'static) -> (String, mpsc::Receiver<Seen>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}/stream", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let callback = |req: &Request, mut res: Response| {
            let headers = req.headers().iter().map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap().to_string())).collect();
            let _ = tx.send(Seen { headers });
            let offered = req.headers().get("sec-websocket-protocol").and_then(|v| v.to_str().ok()).unwrap_or("");
            if let Some(p) = protocol.filter(|p| offered.split(',').any(|o| o.trim() == *p)) {
                res.headers_mut().insert("sec-websocket-protocol", HeaderValue::from_static(p));
            }
            Ok(res)
        };
        let mut ws = tungstenite::accept_hdr(stream, callback).unwrap();
        serve(&mut ws);
    });
    (url, rx)
}

/// Echoes text and binary until the client closes.
fn echo(ws: &mut WebSocket<TcpStream>) {
    while let Ok(msg) = ws.read() {
        if msg.is_text() || msg.is_binary() {
            ws.send(msg).unwrap();
        }
    }
}

/// Closes with `frame`, then reads until the client answers and the stream ends.
fn close_with(frame: Option<CloseFrame>) -> impl FnOnce(&mut WebSocket<TcpStream>) + Send + 'static {
    move |ws| {
        ws.close(frame).unwrap();
        while ws.read().is_ok() {}
    }
}

fn transport() -> SocketTransport {
    IoTaskPool::get_or_init(TaskPool::new);
    SocketTransport::default()
}

pub fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/it/fixtures/tls").join(name)
}

/// The transport's TLS trusting only the test authority.
pub fn test_tls() -> NetTls {
    NetTls::with_roots(&std::fs::read(fixture("ca.pem")).unwrap()).unwrap()
}

/// A server config presenting the `leaf` fixture (`ip` or `localhost`).
pub fn server_tls(leaf: &str) -> Arc<rustls::ServerConfig> {
    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_file_iter(fixture(&format!("{leaf}.pem"))).unwrap().map(Result::unwrap).collect();
    let key = PrivateKeyDer::from_pem_file(fixture(&format!("{leaf}.key"))).unwrap();
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .unwrap();
    Arc::new(config)
}

/// Accepts one TLS connection on `listener` as `leaf`; `None` when the handshake fails.
pub fn accept_tls(listener: &TcpListener, leaf: &str) -> Option<rustls::StreamOwned<rustls::ServerConnection, TcpStream>> {
    let (tcp, _) = listener.accept().unwrap();
    let mut tls = rustls::StreamOwned::new(rustls::ServerConnection::new(server_tls(leaf)).unwrap(), tcp);
    while tls.conn.is_handshaking() {
        tls.conn.complete_io(&mut tls.sock).ok()?;
    }
    Some(tls)
}

/// A `wss:` echo server for one connection, presenting the `ip` leaf; records the handshake.
#[allow(clippy::result_large_err)]
fn wss_echo() -> (String, mpsc::Receiver<Seen>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("wss://{}/stream", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let Some(stream) = accept_tls(&listener, "ip") else { return };
        let callback = |req: &Request, res: Response| {
            let headers = req.headers().iter().map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap().to_string())).collect();
            let _ = tx.send(Seen { headers });
            Ok(res)
        };
        let Ok(mut ws) = tungstenite::accept_hdr(stream, callback) else { return };
        while let Ok(msg) = ws.read() {
            if msg.is_text() || msg.is_binary() {
                ws.send(msg).unwrap();
            }
        }
    });
    (url, rx)
}

fn open(t: &mut SocketTransport, url: &str, protocols: &[&str]) {
    let request = OpenRequest { url: url.to_string(), protocols: protocols.iter().map(|p| p.to_string()).collect(), headers: Vec::new() };
    t.open(ID, &request);
}

/// Polls until an event matches `done`, or panics after 120 s; returns every event so far.
fn until(t: &mut SocketTransport, events: &mut Vec<NetEvent>, done: impl Fn(&NetEvent) -> bool) {
    // Long: a loaded machine can starve the socket threads for many seconds.
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut seen = events.len();
    loop {
        t.poll(events);
        if events[seen..].iter().any(&done) {
            return;
        }
        seen = events.len();
        assert!(Instant::now() < deadline, "timed out; events so far: {events:?}");
        thread::sleep(Duration::from_millis(5));
    }
}

fn is_open(e: &NetEvent) -> bool {
    matches!(e, NetEvent::Open { .. })
}

fn is_close(e: &NetEvent) -> bool {
    matches!(e, NetEvent::Close { .. })
}

fn last_close(events: &[NetEvent]) -> (u16, String, bool) {
    match events.iter().rev().find(|e| is_close(e)) {
        Some(NetEvent::Close { code, reason, clean, .. }) => (*code, reason.clone(), *clean),
        _ => panic!("no close in {events:?}"),
    }
}

#[test]
fn echoes_text_and_binary_and_reports_writes() {
    let (url, _) = server(None, echo);
    let mut t = transport();
    let mut events = Vec::new();
    open(&mut t, &url, &[]);
    until(&mut t, &mut events, is_open);
    assert_eq!(events, vec![NetEvent::Open { id: ID, protocol: String::new(), extensions: String::new() }]);

    t.send(ID, Frame::Text("hello".into()));
    t.send(ID, Frame::Binary(vec![1, 2, 3]));
    until(&mut t, &mut events, |e| matches!(e, NetEvent::Message { frame: Frame::Binary(_), .. }));
    let messages: Vec<&Frame> = events.iter().filter_map(|e| if let NetEvent::Message { frame, .. } = e { Some(frame) } else { None }).collect();
    assert_eq!(messages, vec![&Frame::Text("hello".into()), &Frame::Binary(vec![1, 2, 3])]);
    let written: Vec<usize> = events.iter().filter_map(|e| if let NetEvent::Written { bytes, .. } = e { Some(*bytes) } else { None }).collect();
    assert_eq!(written, vec![5, 3]);

    t.close(ID, Some(1000), "done");
    until(&mut t, &mut events, is_close);
    assert_eq!(last_close(&events), (1000, "done".into(), true));
}

#[test]
fn server_close_with_a_code() {
    let (url, _) = server(None, close_with(Some(CloseFrame { code: CloseCode::Normal, reason: "bye".into() })));
    let mut t = transport();
    let mut events = Vec::new();
    open(&mut t, &url, &[]);
    until(&mut t, &mut events, is_close);
    assert!(is_open(&events[0]));
    assert_eq!(last_close(&events), (1000, "bye".into(), true));
}

#[test]
fn server_close_without_a_code_is_1005() {
    let (url, _) = server(None, close_with(None));
    let mut t = transport();
    let mut events = Vec::new();
    open(&mut t, &url, &[]);
    until(&mut t, &mut events, is_close);
    assert_eq!(last_close(&events), (1005, String::new(), true));
}

#[test]
fn a_dropped_connection_is_1006() {
    let (url, _) = server(None, |ws| {
        ws.get_mut().shutdown(std::net::Shutdown::Both).unwrap();
    });
    let mut t = transport();
    let mut events = Vec::new();
    open(&mut t, &url, &[]);
    until(&mut t, &mut events, is_close);
    assert!(is_open(&events[0]));
    assert_eq!(last_close(&events), (1006, String::new(), false));
}

#[test]
fn a_refused_connection_is_1006() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}/", listener.local_addr().unwrap());
    drop(listener);
    let mut t = transport();
    let mut events = Vec::new();
    open(&mut t, &url, &[]);
    until(&mut t, &mut events, is_close);
    let (code, reason, clean) = last_close(&events);
    assert_eq!((code, clean, events.len()), (1006, false, 1), "{events:?}");
    assert!(reason.contains("the connection failed"), "the reason core logs: {reason}");
}

#[test]
fn a_trusted_local_server() {
    let (url, seen) = wss_echo();
    IoTaskPool::get_or_init(TaskPool::new);
    let mut t = SocketTransport::new(test_tls());
    let mut events = Vec::new();
    // The `TLS` scenario: a key over wss: reaches the server in the handshake.
    let request = OpenRequest { url, protocols: Vec::new(), headers: vec![("authorization".into(), "Bearer k".into())] };
    t.open(ID, &request);
    until(&mut t, &mut events, is_open);
    t.send(ID, Frame::Text("hi".into()));
    until(&mut t, &mut events, |e| matches!(e, NetEvent::Message { .. }));
    assert!(events.contains(&NetEvent::Message { id: ID, frame: Frame::Text("hi".into()) }), "{events:?}");
    let seen = seen.recv().unwrap();
    assert!(seen.headers.iter().any(|(k, v)| k == "authorization" && v == "Bearer k"), "{:?}", seen.headers);
    t.close(ID, Some(1000), "");
    until(&mut t, &mut events, is_close);
    assert_eq!(last_close(&events).0, 1000);
}

#[test]
fn an_untrusted_certificate() {
    let (url, _) = wss_echo();
    // The platform's trust store: the test authority is not in it.
    let mut t = transport();
    let mut events = Vec::new();
    open(&mut t, &url, &[]);
    until(&mut t, &mut events, is_close);
    let (code, reason, clean) = last_close(&events);
    assert_eq!((code, clean), (1006, false), "{events:?}");
    assert!(reason.contains("not trusted"), "{reason}");
    assert!(!events.iter().any(is_open));
}

#[test]
fn open_reports_the_protocol_the_server_chose() {
    let (url, seen) = server(Some("v2"), echo);
    let mut t = transport();
    let mut events = Vec::new();
    open(&mut t, &url, &["v1", "v2"]);
    until(&mut t, &mut events, is_open);
    assert_eq!(events[0], NetEvent::Open { id: ID, protocol: "v2".into(), extensions: String::new() });
    let seen = seen.recv().unwrap();
    let offered = seen.headers.iter().find(|(k, _)| k == "sec-websocket-protocol").map(|(_, v)| v.as_str());
    assert_eq!(offered, Some("v1, v2"));
}

#[test]
fn the_handshake_carries_no_origin_and_the_allowed_headers() {
    let (url, seen) = server(None, echo);
    let mut t = transport();
    let mut events = Vec::new();
    let request = OpenRequest { url, protocols: Vec::new(), headers: vec![("Authorization".into(), "Bearer k".into())] };
    t.open(ID, &request);
    until(&mut t, &mut events, is_open);
    let seen = seen.recv().unwrap();
    assert!(!seen.headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("origin")), "{:?}", seen.headers);
    assert!(seen.headers.iter().any(|(k, v)| k == "authorization" && v == "Bearer k"));
}

#[test]
fn a_20_mib_message_closes_with_1009() {
    let (url, _) = server(None, |ws| {
        let _ = ws.send(Message::binary(vec![0u8; 20 << 20]));
        while ws.read().is_ok() {}
    });
    let mut t = transport();
    let mut events = Vec::new();
    open(&mut t, &url, &[]);
    until(&mut t, &mut events, is_close);
    assert!(!events.iter().any(|e| matches!(e, NetEvent::Message { .. })));
    assert_eq!(last_close(&events), (1009, "message too big".into(), false));
}

#[test]
fn close_while_connecting_abandons_it() {
    // A listener that never accepts: the handshake never finishes.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}/", listener.local_addr().unwrap());
    let mut t = transport();
    let mut events = Vec::new();
    open(&mut t, &url, &[]);
    t.close(ID, None, "");
    until(&mut t, &mut events, is_close);
    assert_eq!(events, vec![NetEvent::Close { id: ID, code: 1006, reason: String::new(), clean: false }]);
    drop(listener);
}
