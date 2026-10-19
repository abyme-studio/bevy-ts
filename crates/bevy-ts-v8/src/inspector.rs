//! The V8 inspector over the Chrome DevTools Protocol, for `bevy-ts dev --inspect`.
//!
//! A thread serves `/json/list` and `/json/version` for discovery and one websocket per
//! debugger session, bridged to the isolate's inspector through deno_core's session channels.
//! The isolate stays on the main thread: messages are dispatched when the backend pumps, and
//! while JS is paused at a breakpoint deno_core's pause loop dispatches them, blocking the frame.

use crate::V8Backend;
use deno_core::futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use deno_core::{InspectorMsg, InspectorSessionChannels, InspectorSessionKind, InspectorSessionProxy};
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub(crate) struct InspectorServer {
    address: SocketAddr,
    stop: Arc<AtomicBool>,
}

impl InspectorServer {
    pub(crate) fn address(&self) -> SocketAddr {
        self.address
    }
}

impl Drop for InspectorServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // Wake the accept loop so it sees the flag.
        let _ = TcpStream::connect(self.address);
    }
}

pub(crate) const TARGET_ID: &str = "bevy-ts-0000-0000-0000-000000000001";

fn http(stream: &mut TcpStream, body: &str) {
    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=UTF-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

fn targets(address: SocketAddr, title: &str) -> String {
    let ws = format!("{address}/{TARGET_ID}");
    serde_json::json!([{
        "description": "bevy-ts",
        "devtoolsFrontendUrl": format!("devtools://devtools/bundled/js_app.html?experiments=true&v8only=true&ws={ws}"),
        "id": TARGET_ID,
        "title": title,
        "type": "node",
        "url": title,
        "webSocketDebuggerUrl": format!("ws://{ws}"),
    }])
    .to_string()
}

fn serve(listener: TcpListener, address: SocketAddr, title: String, sessions: UnboundedSender<InspectorSessionProxy>, stop: Arc<AtomicBool>) {
    for stream in listener.incoming() {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        let Ok(mut stream) = stream else { continue };
        let mut first = String::new();
        let peeked = {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let _ = reader.read_line(&mut first);
            // Leave the rest for tungstenite: re-read the request from a peek is not possible,
            // so discovery requests are answered here and websocket requests are re-parsed by
            // reading the remaining headers.
            let mut headers = String::new();
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                headers.push_str(&line);
            }
            headers
        };
        let path = first.split_whitespace().nth(1).unwrap_or("/").to_string();
        if path.starts_with("/json/version") {
            http(&mut stream, r#"{"Browser":"bevy-ts","Protocol-Version":"1.3"}"#);
        } else if path.starts_with("/json") {
            http(&mut stream, &targets(address, &title));
        } else if peeked.to_ascii_lowercase().contains("upgrade: websocket") {
            let key = peeked
                .lines()
                .find_map(|l| l.split_once(':').filter(|(k, _)| k.eq_ignore_ascii_case("sec-websocket-key")).map(|(_, v)| v.trim().to_string()));
            let Some(key) = key else { continue };
            let accept = tungstenite::handshake::derive_accept_key(key.as_bytes());
            let _ = write!(
                stream,
                "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
            );
            let ws = tungstenite::WebSocket::from_raw_socket(stream, tungstenite::protocol::Role::Server, None);
            let sessions = sessions.clone();
            std::thread::Builder::new().name("bevy-ts-cdp".into()).spawn(move || session(ws, sessions)).ok();
        }
    }
}

/// Bridges one websocket to one inspector session.
fn session(mut ws: tungstenite::WebSocket<TcpStream>, sessions: UnboundedSender<InspectorSessionProxy>) {
    let (to_frontend, mut from_inspector): (UnboundedSender<InspectorMsg>, UnboundedReceiver<InspectorMsg>) = unbounded();
    let (to_inspector, from_frontend): (UnboundedSender<String>, UnboundedReceiver<String>) = unbounded();
    let proxy = InspectorSessionProxy {
        channels: InspectorSessionChannels::Regular { tx: to_frontend, rx: from_frontend },
        kind: InspectorSessionKind::NonBlocking { wait_for_disconnect: false },
    };
    if sessions.unbounded_send(proxy).is_err() {
        return;
    }
    let _ = ws.get_mut().set_read_timeout(Some(Duration::from_millis(5)));
    loop {
        while let Ok(msg) = from_inspector.try_recv() {
            if ws.send(tungstenite::Message::text(msg.content)).is_err() {
                return;
            }
        }
        match ws.read() {
            Ok(tungstenite::Message::Text(t)) => {
                if to_inspector.unbounded_send(t.to_string()).is_err() {
                    return;
                }
            }
            Ok(tungstenite::Message::Close(_)) => return,
            Ok(_) => {}
            Err(tungstenite::Error::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
            Err(_) => return,
        }
    }
}

impl V8Backend {
    pub(crate) fn start_inspector(&mut self) {
        let Some(address) = self.options.inspect else { return };
        let listener = match TcpListener::bind(address) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("bevy-ts: cannot serve the inspector on {address}: {e}");
                return;
            }
        };
        let address = listener.local_addr().unwrap_or(address);
        let sessions = self.runtime.inspector().get_session_sender();
        let stop = Arc::new(AtomicBool::new(false));
        let s = stop.clone();
        let title = "bevy-ts".to_string();
        std::thread::Builder::new()
            .name("bevy-ts-inspector".into())
            .spawn(move || serve(listener, address, title, sessions, s))
            .expect("spawn the inspector thread");
        eprintln!("bevy-ts: debugger listening on ws://{address}/{TARGET_ID}");
        self.inspector = Some(InspectorServer { address, stop });
    }

    pub(crate) fn stop_inspector(&mut self) {
        self.inspector = None;
    }

    pub(crate) fn pump_inspector(&mut self) {
        if self.inspector.is_some() {
            let waker = std::task::Waker::noop();
            let mut cx = std::task::Context::from_waker(waker);
            self.runtime.inspector().poll_sessions_from_event_loop(&mut cx);
        }
    }
}
