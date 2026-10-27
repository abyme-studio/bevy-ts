//! Change script-networking-2: `SocketTransport`'s fetches against a local HTTP/1.1 server on an
//! ephemeral port (std `TcpListener`, hand-written answers), in clear and over TLS with the test
//! authority. One exchange each; redirects are core's (`net.rs`).

use crate::socket::{accept_tls, refused_addr, test_tls};
use bevy::tasks::{IoTaskPool, TaskPool};
use bevy_ts::net::{FetchId, HttpRequest, HttpResponse, NetEvent, NetTls, SocketTransport, Transport};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const ID: FetchId = FetchId(1);

/// What the server read: the request line, lower-case headers, the body.
#[derive(Debug)]
struct Seen {
    line: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Seen {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }
}

/// Reads one request: its head, then a `content-length` body.
fn read_request<R: Read + ?Sized>(stream: &mut R) -> Option<Seen> {
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    while !buf.ends_with(b"\r\n\r\n") {
        if stream.read(&mut byte).ok()? == 0 {
            return None;
        }
        buf.push(byte[0]);
    }
    let head = String::from_utf8(buf).unwrap();
    let mut lines = head.split("\r\n");
    let line = lines.next().unwrap().to_string();
    let headers: Vec<(String, String)> = lines.filter(|l| !l.is_empty()).map(|l| l.split_once(':').unwrap()).map(|(k, v)| (k.to_ascii_lowercase(), v.trim().to_string())).collect();
    let len = headers.iter().find(|(k, _)| k == "content-length").map_or(0, |(_, v)| v.parse().unwrap());
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body).ok()?;
    Some(Seen { line, headers, body })
}

/// Serves one exchange on a thread, in clear or (`tls`: a leaf's name) over TLS: reads the
/// request, writes `answer` as it is, closes. Returns the base URL and the request it saw.
fn server(tls: Option<&'static str>, answer: Vec<u8>) -> (String, mpsc::Receiver<Seen>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let scheme = if tls.is_some() { "https" } else { "http" };
    let url = format!("{scheme}://{}", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let serve = |stream: &mut dyn ReadWrite| {
            let Some(seen) = read_request(stream) else { return };
            let _ = tx.send(seen);
            let _ = stream.write_all(&answer);
            let _ = stream.flush();
        };
        match tls {
            Some(leaf) => {
                if let Some(mut s) = accept_tls(&listener, leaf) {
                    serve(&mut s);
                    s.conn.send_close_notify();
                    let _ = s.conn.complete_io(&mut s.sock);
                }
            }
            None => serve(&mut listener.accept().unwrap().0),
        }
    });
    (url, rx)
}

trait ReadWrite: Read + Write {}
impl<T: Read + Write> ReadWrite for T {}

fn transport() -> SocketTransport {
    IoTaskPool::get_or_init(TaskPool::new);
    SocketTransport::new(test_tls())
}

fn request(method: &str, url: &str, headers: &[(&str, &str)], body: &[u8]) -> HttpRequest {
    HttpRequest { method: method.into(), url: url.into(), headers: headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(), body: body.to_vec() }
}

/// Polls until the fetch's one event, or panics after 120 s (a loaded machine is slow).
fn outcome(t: &mut SocketTransport) -> Result<HttpResponse, String> {
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut events = Vec::new();
    loop {
        t.poll(&mut events);
        match events.pop() {
            Some(NetEvent::Response { response, .. }) => return Ok(response),
            Some(NetEvent::FetchFailed { reason, .. }) => return Err(reason),
            Some(other) => panic!("not a fetch event: {other:?}"),
            None => {}
        }
        assert!(Instant::now() < deadline, "timed out");
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn the_abyme_client_reads_what_is_held() {
    let (base, seen) = server(None, b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 11\r\n\r\n{\"held\":[]}".to_vec());
    let mut t = transport();
    let url = format!("{base}/api/held");
    t.fetch(ID, &request("GET", &url, &[("authorization", "Bearer k"), ("origin", "http://evil"), ("accept-encoding", "gzip"), ("host", "evil")], b""));
    let r = outcome(&mut t).unwrap();
    assert_eq!((r.status, r.reason.as_str(), r.body.as_slice()), (200, "OK", &br#"{"held":[]}"#[..]));
    assert_eq!(r.header("content-type"), Some("application/json"));
    let seen = seen.recv().unwrap();
    assert_eq!(seen.line, "GET /api/held HTTP/1.1");
    assert_eq!(seen.header("authorization"), Some("Bearer k"));
    assert_eq!(seen.header("accept-encoding"), Some("identity"), "the engine's, not the script's");
    assert_eq!(seen.header("origin"), None);
    assert_eq!(seen.header("host"), Some(base.trim_start_matches("http://")));
    assert_eq!(seen.header("connection"), Some("close"));
    assert_eq!(seen.header("accept"), Some("*/*"));
    assert!(seen.header("user-agent").is_some_and(|u| u.starts_with("bevy-ts/")), "{seen:?}");
}

#[test]
fn a_post_with_a_json_body() {
    let (base, seen) = server(None, b"HTTP/1.1 201 Created\r\ncontent-length: 0\r\n\r\n".to_vec());
    let mut t = transport();
    t.fetch(ID, &request("POST", &format!("{base}/a?x=1"), &[("content-type", "text/plain;charset=UTF-8"), ("content-length", "99")], br#"{"a":1}"#));
    assert_eq!(outcome(&mut t).unwrap().status, 201);
    let seen = seen.recv().unwrap();
    assert_eq!(seen.line, "POST /a?x=1 HTTP/1.1");
    assert_eq!(seen.header("content-type"), Some("text/plain;charset=UTF-8"));
    assert_eq!(seen.header("content-length"), Some("7"));
    assert_eq!(seen.body, br#"{"a":1}"#);
}

#[test]
fn chunked_and_close_delimited_bodies() {
    let (base, _) = server(None, b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n".to_vec());
    let mut t = transport();
    t.fetch(ID, &request("GET", &base, &[], b""));
    assert_eq!(outcome(&mut t).unwrap().body, b"hello world");
    let (base, _) = server(None, b"HTTP/1.1 200 OK\r\n\r\nuntil the end".to_vec());
    t.fetch(FetchId(2), &request("GET", &base, &[], b""));
    assert_eq!(outcome(&mut t).unwrap().body, b"until the end");
}

#[test]
fn nothing_listening() {
    let url = format!("http://{}/", refused_addr());
    let mut t = transport();
    t.fetch(ID, &request("GET", &url, &[], b""));
    let e = outcome(&mut t).unwrap_err();
    assert!(e.contains("the connection failed"), "{e}");
}

#[test]
fn https_with_a_trusted_certificate() {
    let (base, seen) = server(Some("ip"), b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nok".to_vec());
    let mut t = transport();
    t.fetch(ID, &request("GET", &format!("{base}/secure"), &[], b""));
    assert_eq!(outcome(&mut t).unwrap().body, b"ok");
    assert_eq!(seen.recv().unwrap().line, "GET /secure HTTP/1.1");
}

#[test]
fn the_wrong_name() {
    // The certificate names only localhost; the URL's host is 127.0.0.1.
    let (base, _) = server(Some("localhost"), Vec::new());
    let mut t = transport();
    t.fetch(ID, &request("GET", &base, &[], b""));
    let e = outcome(&mut t).unwrap_err();
    assert!(e.contains("the certificate does not match 127.0.0.1"), "{e}");
}

#[test]
fn a_tls_config_that_cannot_be_built() {
    // As the platform's on a Linux machine with no certificates: that fetch fails with the
    // reason, opens no socket, and a fetch in clear still works.
    IoTaskPool::get_or_init(TaskPool::new);
    let mut t = SocketTransport::new(NetTls::built_by(|| Err("no certificates here".into())));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    t.fetch(ID, &request("GET", &format!("https://{}/", listener.local_addr().unwrap()), &[], b""));
    let e = outcome(&mut t).unwrap_err();
    assert!(e.contains("no certificates here"), "{e}");
    assert!(listener.accept().is_err(), "no connection was made");
    let (base, _) = server(None, b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nok".to_vec());
    t.fetch(FetchId(2), &request("GET", &base, &[], b""));
    assert_eq!(outcome(&mut t).unwrap().body, b"ok");
}

#[test]
fn a_huge_download_stops_at_the_bound() {
    let mut answer = b"HTTP/1.1 200 OK\r\ncontent-length: 20971520\r\n\r\n".to_vec();
    answer.resize(answer.len() + (20 << 20), b'x');
    let (base, _) = server(None, answer);
    let mut t = transport();
    t.fetch(ID, &request("GET", &base, &[], b""));
    let e = outcome(&mut t).unwrap_err();
    assert!(e.contains("over the 16 MiB limit"), "{e}");
    // With no length, the bound stops the read.
    let mut answer = b"HTTP/1.1 200 OK\r\n\r\n".to_vec();
    answer.resize(answer.len() + (20 << 20), b'x');
    let (base, _) = server(None, answer);
    t.fetch(FetchId(2), &request("GET", &base, &[], b""));
    let e = outcome(&mut t).unwrap_err();
    assert!(e.contains("over the 16 MiB limit"), "{e}");
}

#[test]
fn a_cancel_closes_the_servers_connection() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/never", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream).unwrap();
        tx.send("read").unwrap();
        // Never answers: the next read ends when the client goes.
        let mut rest = [0u8; 16];
        let n = stream.read(&mut rest).unwrap_or(0);
        tx.send(if n == 0 { "closed" } else { "more" }).unwrap();
    });
    let mut t = transport();
    t.fetch(ID, &request("GET", &url, &[], b""));
    assert_eq!(rx.recv_timeout(Duration::from_secs(120)).unwrap(), "read");
    t.cancel(ID);
    assert_eq!(rx.recv_timeout(Duration::from_secs(120)).unwrap(), "closed");
    let mut events = Vec::new();
    t.poll(&mut events);
    assert!(events.is_empty(), "a cancelled fetch sends nothing: {events:?}");
}
