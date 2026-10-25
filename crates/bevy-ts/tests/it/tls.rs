//! `wss:` and the header rule: `NetRegistry` (policy above it) over a `SocketTransport` against a
//! local rustls server on an ephemeral port, its certificate issued by a test root.

use bevy::tasks::{IoTaskPool, TaskPool};
use bevy_ts::net::{ConnId, Frame, HeadersOrigin, NetEvent, NetPolicy, NetRegistry, NetRequest, OpenRequest, SocketTransport};
use rcgen::{BasicConstraints, CertificateParams, CertifiedIssuer, IsCa, KeyPair};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};
use std::net::{SocketAddr, TcpListener};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};
use tungstenite::handshake::server::{Request, Response};

/// The spec's host: resolved to the local server by `with_resolve`, never by DNS.
const HOST: &str = "mac.tail.ts.net";
const ID: ConnId = ConnId(1);

/// A test root, and a certificate for `HOST` it issued: `(root DER, leaf DER, leaf key)`.
fn issued() -> (CertificateDer<'static>, CertificateDer<'static>, PrivateKeyDer<'static>) {
    let mut ca = CertificateParams::new(Vec::<String>::new()).unwrap();
    ca.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    let ca = CertifiedIssuer::self_signed(ca, KeyPair::generate().unwrap()).unwrap();
    let key = KeyPair::generate().unwrap();
    let leaf = CertificateParams::new(vec![HOST.to_string()]).unwrap().signed_by(&key, &ca).unwrap();
    (ca.der().clone(), leaf.der().clone(), PrivatePkcs8KeyDer::from(key.serialize_der()).into())
}

/// A self-signed certificate for `HOST` no root vouches for.
fn self_signed() -> (CertificateDer<'static>, PrivateKeyDer<'static>) {
    let c = rcgen::generate_simple_self_signed(vec![HOST.to_string()]).unwrap();
    (c.cert.der().clone(), PrivatePkcs8KeyDer::from(c.signing_key.serialize_der()).into())
}

/// Serves one TLS connection on a thread with `cert`: records the handshake's headers, then
/// echoes until the client goes. Returns the server's address and the headers it saw.
// tungstenite's `accept_hdr` callback returns its own large `ErrorResponse`.
#[allow(clippy::result_large_err)]
fn server(cert: CertificateDer<'static>, key: PrivateKeyDer<'static>) -> (SocketAddr, mpsc::Receiver<Vec<(String, String)>>) {
    let config = ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![cert], key)
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let (tcp, _) = listener.accept().unwrap();
        let tls = StreamOwned::new(ServerConnection::new(Arc::new(config)).unwrap(), tcp);
        let callback = |req: &Request, res: Response| {
            let headers = req.headers().iter().map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap().to_string())).collect();
            let _ = tx.send(headers);
            Ok(res)
        };
        // A client that refuses the certificate fails the handshake: nothing to serve.
        let Ok(mut ws) = tungstenite::accept_hdr(tls, callback) else { return };
        while let Ok(msg) = ws.read() {
            if msg.is_text() {
                ws.send(msg).unwrap();
            }
        }
    });
    (addr, rx)
}

/// `net.allow` has `HOST:port` and `other.example.com`; `headersOrigin` is `https://HOST:port`.
fn policy(port: u16) -> NetPolicy {
    let allow = [format!("{HOST}:{port}"), "other.example.com".to_string()];
    let (mut policy, warnings) = NetPolicy::from_config(&allow, &[], &|_| None);
    assert!(warnings.is_empty(), "{warnings:?}");
    policy.headers_origin = Some(HeadersOrigin::parse(&format!("https://{HOST}:{port}")).unwrap());
    policy
}

fn registry(transport: SocketTransport) -> NetRegistry {
    IoTaskPool::get_or_init(TaskPool::new);
    NetRegistry::new(Box::new(transport))
}

fn open(net: &mut NetRegistry, policy: &NetPolicy, url: &str, headers: &[(&str, &str)]) -> Vec<String> {
    let headers = headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    let request = OpenRequest { url: url.into(), protocols: Vec::new(), headers };
    net.apply(vec![NetRequest::Open { id: ID, generation: 0, request }], 0, policy, false)
}

/// Takes batches until an event matches `done`, or panics after 120 s; collects events and log lines.
fn until(net: &mut NetRegistry, events: &mut Vec<NetEvent>, log: &mut Vec<String>, done: impl Fn(&NetEvent) -> bool) {
    // Long: a loaded machine can starve the socket threads for many seconds.
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let (batch, lines) = net.take_batch();
        log.extend(lines);
        let hit = batch.iter().any(&done);
        events.extend(batch);
        if hit {
            return;
        }
        assert!(Instant::now() < deadline, "timed out; events so far: {events:?}, log: {log:?}");
        thread::sleep(Duration::from_millis(5));
    }
}

fn is_close(e: &NetEvent) -> bool {
    matches!(e, NetEvent::Close { .. })
}

fn refused(events: &[NetEvent]) -> bool {
    events == [NetEvent::Error { id: ID }, NetEvent::Close { id: ID, code: 1006, reason: String::new(), clean: false }]
}

#[test]
fn wss_opens_and_carries_headers_to_the_named_origin() {
    let (root, cert, key) = issued();
    let (addr, seen) = server(cert, key);
    let policy = policy(addr.port());
    let mut net = registry(SocketTransport::default().with_root(root.to_vec()).with_resolve(HOST, addr));
    let mut events = Vec::new();
    let mut log = open(&mut net, &policy, &format!("wss://{HOST}:{}/api/stream", addr.port()), &[("authorization", "Bearer k")]);
    assert!(log.is_empty(), "{log:?}");
    until(&mut net, &mut events, &mut log, |e| matches!(e, NetEvent::Open { .. }));
    let headers = seen.recv_timeout(Duration::from_secs(120)).unwrap();
    assert!(headers.iter().any(|(k, v)| k == "authorization" && v == "Bearer k"), "{headers:?}");
    assert!(headers.iter().any(|(k, v)| k == "host" && v.starts_with(HOST)), "{headers:?}");

    net.apply(vec![NetRequest::Send { id: ID, generation: 0, frame: Frame::Text("over tls".into()) }], 0, &policy, false);
    until(&mut net, &mut events, &mut log, |e| matches!(e, NetEvent::Message { .. }));
    assert!(events.contains(&NetEvent::Message { id: ID, frame: Frame::Text("over tls".into()) }));
    assert!(log.is_empty(), "{log:?}");
}

#[test]
fn a_certificate_no_root_vouches_for_is_1006_and_logged() {
    let (cert, key) = self_signed();
    let (addr, _seen) = server(cert, key);
    let policy = policy(addr.port());
    let mut net = registry(SocketTransport::default().with_resolve(HOST, addr));
    let mut events = Vec::new();
    let url = format!("wss://{HOST}:{}/", addr.port());
    let mut log = open(&mut net, &policy, &url, &[]);
    until(&mut net, &mut events, &mut log, is_close);
    assert!(refused(&events), "{events:?}");
    let line = log.iter().find(|l| l.contains("TLS failed")).unwrap_or_else(|| panic!("no TLS line in {log:?}"));
    assert!(line.starts_with(&format!("WebSocket {url}: ")), "{line}");
    assert!(line.contains("invalid peer certificate"), "{line}");
}

#[test]
fn headers_off_loopback_go_only_over_wss_to_the_named_origin() {
    let policy = policy(8443);
    let expect = "headers are sent only to loopback or the host's named origin over wss:";
    for url in [format!("ws://{HOST}:8443/api/stream"), "wss://other.example.com/".to_string()] {
        // No root and no resolve: a refused open must never reach DNS or a socket.
        let mut net = registry(SocketTransport::default());
        let mut events = Vec::new();
        let mut log = open(&mut net, &policy, &url, &[("authorization", "Bearer k")]);
        assert_eq!(log, vec![format!("WebSocket {url}: {expect}")]);
        until(&mut net, &mut events, &mut log, is_close);
        assert!(refused(&events), "{url}: {events:?}");
        assert_eq!(net.len(), 0);
    }
}
