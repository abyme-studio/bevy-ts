//! A transport with no socket and no thread: tests accept or refuse opens, push messages, close
//! or drop from the server side, and read what scripts sent.

use super::{ConnId, Frame, NetEvent, OpenRequest, Transport};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

/// One connection as the fake server sees it.
#[derive(Clone, Debug, Default)]
pub struct FakeConn {
    pub request: OpenRequest,
    /// Frames the script sent, in order.
    pub sent: Vec<Frame>,
    /// The close the script or the engine asked for: `(code, reason)`.
    pub closed: Option<(Option<u16>, String)>,
    pub accepted: bool,
}

#[derive(Default)]
struct State {
    conns: BTreeMap<ConnId, FakeConn>,
    events: Vec<NetEvent>,
}

/// The fake transport; clones share one state, so a test keeps a handle after installing it.
#[derive(Clone, Default)]
pub struct FakeTransport(Arc<Mutex<State>>);

impl FakeTransport {
    /// Every connection opened so far, by id.
    pub fn conns(&self) -> BTreeMap<ConnId, FakeConn> {
        self.0.lock().unwrap().conns.clone()
    }

    /// The connection last opened.
    pub fn last(&self) -> Option<(ConnId, FakeConn)> {
        self.0.lock().unwrap().conns.iter().next_back().map(|(id, c)| (*id, c.clone()))
    }

    pub fn conn(&self, id: ConnId) -> Option<FakeConn> {
        self.0.lock().unwrap().conns.get(&id).cloned()
    }

    /// Completes the handshake, choosing `protocol` (empty for none).
    pub fn accept(&self, id: ConnId, protocol: &str) {
        let mut s = self.0.lock().unwrap();
        if let Some(c) = s.conns.get_mut(&id) {
            c.accepted = true;
        }
        s.events.push(NetEvent::Open { id, protocol: protocol.into(), extensions: String::new() });
    }

    /// Fails the handshake, as a refused TCP connection or a `403` would.
    pub fn refuse(&self, id: ConnId) {
        self.drop_conn(id);
    }

    pub fn push_text(&self, id: ConnId, text: &str) {
        self.0.lock().unwrap().events.push(NetEvent::Message { id, frame: Frame::Text(text.into()) });
    }

    pub fn push_binary(&self, id: ConnId, bytes: &[u8]) {
        self.0.lock().unwrap().events.push(NetEvent::Message { id, frame: Frame::Binary(bytes.to_vec()) });
    }

    /// A close frame from the server: `None` for one without a status (scripts see 1005).
    pub fn server_close(&self, id: ConnId, code: Option<u16>, reason: &str) {
        let code = code.unwrap_or(1005);
        self.0.lock().unwrap().events.push(NetEvent::Close { id, code, reason: reason.into(), clean: true });
    }

    /// The TCP connection drops without a close frame (scripts see 1006).
    pub fn drop_conn(&self, id: ConnId) {
        self.0.lock().unwrap().events.push(NetEvent::Close { id, code: super::ABNORMAL, reason: String::new(), clean: false });
    }

    /// The I/O side wrote `bytes` of the connection's sends.
    pub fn written(&self, id: ConnId, bytes: usize) {
        self.0.lock().unwrap().events.push(NetEvent::Written { id, bytes });
    }
}

impl Transport for FakeTransport {
    fn open(&mut self, id: ConnId, request: &OpenRequest) {
        self.0.lock().unwrap().conns.insert(id, FakeConn { request: request.clone(), ..Default::default() });
    }

    fn send(&mut self, id: ConnId, frame: Frame) {
        if let Some(c) = self.0.lock().unwrap().conns.get_mut(&id) {
            c.sent.push(frame);
        }
    }

    /// The server answers a script's close at once, as a well-behaved one does; a connection
    /// not yet accepted is abandoned (1006), as the socket transport does.
    fn close(&mut self, id: ConnId, code: Option<u16>, reason: &str) {
        let mut s = self.0.lock().unwrap();
        let Some(c) = s.conns.get_mut(&id) else { return };
        if c.closed.is_some() {
            return;
        }
        c.closed = Some((code, reason.into()));
        let event = if c.accepted {
            NetEvent::Close { id, code: code.unwrap_or(1005), reason: reason.into(), clean: true }
        } else {
            NetEvent::Close { id, code: super::ABNORMAL, reason: String::new(), clean: false }
        };
        s.events.push(event);
    }

    fn poll(&mut self, out: &mut Vec<NetEvent>) {
        out.append(&mut self.0.lock().unwrap().events);
    }
}
