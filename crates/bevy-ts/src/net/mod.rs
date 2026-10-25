//! Script networking: the `WebSocket` connections scripts open, owned here, above any JS engine.
//!
//! A backend holds no socket. Scripts' requests (open, send, close) cross the seam as plain data
//! tagged with a connection id and a generation; [`net_deliver`] applies them through
//! [`NetPolicy`] to a [`Transport`] (sockets, or [`FakeTransport`] in tests) and hands what the
//! transport received back to the backend in one call a frame. The allowlist, the limits and
//! the generation rules live here, so the fake transport is held to them too.

mod fake;
pub mod policy;
mod socket;

pub use fake::{FakeConn, FakeTransport};
pub use policy::{HeadersOrigin, NetPolicy, Refusal};
pub use socket::SocketTransport;

use crate::log::{LogLevel, ScriptLog};
use crate::runtime::{ScriptRuntime, current_generation};
use bevy::prelude::*;
use std::collections::HashMap;

/// A connection, numbered by the script side; unique within one engine instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ConnId(pub u64);

/// One message's payload: a text frame or a binary one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Frame {
    Text(String),
    Binary(Vec<u8>),
}

impl Frame {
    pub fn len(&self) -> usize {
        match self {
            Frame::Text(s) => s.len(),
            Frame::Binary(b) => b.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// What a script asked to open.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OpenRequest {
    /// A `ws:` or `wss:` URL, as the script side normalised it.
    pub url: String,
    pub protocols: Vec<String>,
    /// The non-standard, Deno-shaped `headers` option: to loopback, or over `wss:` to the
    /// host's named origin.
    pub headers: Vec<(String, String)>,
}

/// A request a script made, in the order it made them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NetRequest {
    Open { id: ConnId, generation: u64, request: OpenRequest },
    Send { id: ConnId, generation: u64, frame: Frame },
    /// `code` is `None` for `close()` without one: the close frame carries no status.
    Close { id: ConnId, generation: u64, code: Option<u16>, reason: String },
}

impl NetRequest {
    pub fn generation(&self) -> u64 {
        match self {
            NetRequest::Open { generation, .. } | NetRequest::Send { generation, .. } | NetRequest::Close { generation, .. } => *generation,
        }
    }
}

/// Something that happened to a connection, delivered to scripts in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NetEvent {
    Open { id: ConnId, protocol: String, extensions: String },
    Message { id: ConnId, frame: Frame },
    /// The web's plain `error` event; core sends one before every unclean close.
    Error { id: ConnId },
    /// `code` 1005 for a close frame without a status, 1006 for a dropped connection.
    Close { id: ConnId, code: u16, reason: String, clean: bool },
    /// The I/O side wrote this many bytes of the connection's sends (`bufferedAmount` falls).
    Written { id: ConnId, bytes: usize },
}

impl NetEvent {
    pub fn id(&self) -> ConnId {
        match self {
            NetEvent::Open { id, .. }
            | NetEvent::Message { id, .. }
            | NetEvent::Error { id }
            | NetEvent::Close { id, .. }
            | NetEvent::Written { id, .. } => *id,
        }
    }
}

/// Where connections really go. Core calls it on the main thread; an implementation does its
/// I/O elsewhere and hands back what arrived on [`Transport::poll`].
pub trait Transport {
    /// Starts connecting. The policy has already allowed it; no `Origin` header may be added.
    fn open(&mut self, id: ConnId, request: &OpenRequest);
    fn send(&mut self, id: ConnId, frame: Frame);
    /// Closes with a close frame (`code` `None`: one without a status), or drops a connection
    /// still connecting. Its `close` event, if any, is no longer wanted once core forgot the id.
    fn close(&mut self, id: ConnId, code: Option<u16>, reason: &str);
    /// Appends every event received since the last poll, each connection's in order.
    fn poll(&mut self, out: &mut Vec<NetEvent>);
    /// Appends warnings for the log (a TLS error that failed an open), each a whole line. Called
    /// after [`Transport::poll`]: a warning is ready by the time its connection's `close` is.
    fn take_log(&mut self, _out: &mut Vec<String>) {}
}

/// Messages a connection may hold undelivered.
pub const MAX_QUEUED_MESSAGES: usize = 4096;
/// Bytes a connection may hold undelivered.
pub const MAX_QUEUED_BYTES: usize = 64 << 20;
/// The largest message a connection receives.
pub const MAX_MESSAGE_BYTES: usize = 16 << 20;

/// Close codes the engine itself uses.
pub const GOING_AWAY: u16 = 1001;
pub const ABNORMAL: u16 = 1006;
pub const TOO_BIG: u16 = 1009;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnState {
    Connecting,
    Open,
}

/// A connection core knows of.
#[derive(Clone, Debug)]
pub struct Conn {
    /// The id the transport knows it by: unique for the whole run, while script ids start
    /// again in a fresh engine instance.
    pub wire: ConnId,
    pub generation: u64,
    pub state: ConnState,
    /// The origin as the log names it (`127.0.0.1:4477`).
    pub origin: String,
}

/// The connections scripts hold, and the transport they go through. Non-send: it lives with
/// the runtime on the main thread.
pub struct NetRegistry {
    transport: Box<dyn Transport>,
    conns: HashMap<ConnId, Conn>,
    /// Transport ids to script ids.
    wires: HashMap<ConnId, ConnId>,
    next_wire: u64,
    /// Events core made itself (refusals, edit mode's closes), delivered first next time.
    pending: Vec<NetEvent>,
    /// The batch being built, reused across frames.
    batch: Vec<NetEvent>,
    polled: Vec<NetEvent>,
}

impl NetRegistry {
    pub fn new(transport: Box<dyn Transport>) -> Self {
        NetRegistry { transport, conns: HashMap::new(), wires: HashMap::new(), next_wire: 1, pending: Vec::new(), batch: Vec::new(), polled: Vec::new() }
    }

    pub fn conns(&self) -> impl Iterator<Item = (ConnId, &Conn)> {
        self.conns.iter().map(|(id, c)| (*id, c))
    }

    pub fn len(&self) -> usize {
        self.conns.len()
    }

    pub fn is_empty(&self) -> bool {
        self.conns.is_empty()
    }

    /// Closes the connection with `code` and forgets it: scripts hear nothing more of it.
    fn close_quietly(&mut self, id: ConnId, code: u16, reason: &str) {
        if let Some(c) = self.forget(id) {
            self.transport.close(c.wire, Some(code), reason);
        }
    }

    fn forget(&mut self, id: ConnId) -> Option<Conn> {
        let c = self.conns.remove(&id)?;
        self.wires.remove(&c.wire);
        Some(c)
    }

    /// Closes every connection of a generation other than `keep` with 1001, delivering nothing
    /// to them: a committed generation replaced theirs.
    pub fn close_other_generations(&mut self, keep: u64) {
        let old: Vec<ConnId> = self.conns.iter().filter(|(_, c)| c.generation != keep).map(|(id, _)| *id).collect();
        for id in old {
            self.close_quietly(id, GOING_AWAY, "going away");
        }
        self.pending.clear();
    }

    /// Closes every connection with 1001 and forgets every pending event: a fresh engine
    /// instance knows none of them.
    pub fn close_all(&mut self) {
        let all: Vec<ConnId> = self.conns.keys().copied().collect();
        for id in all {
            self.close_quietly(id, GOING_AWAY, "going away");
        }
        self.pending.clear();
    }

    /// Closes every connection with 1001 and queues its `close` for the next delivery: the
    /// game left play.
    pub fn close_for_edit(&mut self) {
        let mut all: Vec<ConnId> = self.conns.keys().copied().collect();
        all.sort();
        for id in all {
            self.close_quietly(id, GOING_AWAY, "going away");
            self.pending.push(NetEvent::Close { id, code: GOING_AWAY, reason: "going away".into(), clean: true });
        }
    }

    /// A refused open: `error`, then `close` with 1006, at the next delivery.
    fn refuse(&mut self, id: ConnId) {
        self.pending.push(NetEvent::Error { id });
        self.pending.push(NetEvent::Close { id, code: ABNORMAL, reason: String::new(), clean: false });
    }

    /// Applies scripts' requests: opens through the policy, sends and closes to known
    /// connections. Requests of a generation other than `live` are dropped; that generation's
    /// script side is gone. Returns log lines for refusals.
    pub fn apply(&mut self, requests: Vec<NetRequest>, live: u64, policy: &NetPolicy, edit: bool) -> Vec<String> {
        let mut log = Vec::new();
        for r in requests {
            if r.generation() != live {
                continue;
            }
            match r {
                NetRequest::Open { id, generation, request } => match policy.check(&request.url, &request.headers, edit) {
                    Ok(origin) => {
                        let wire = ConnId(self.next_wire);
                        self.next_wire += 1;
                        self.conns.insert(id, Conn { wire, generation, state: ConnState::Connecting, origin });
                        self.wires.insert(wire, id);
                        self.transport.open(wire, &request);
                    }
                    Err(refusal) => {
                        log.push(format!("WebSocket {}: {refusal}", request.url));
                        self.refuse(id);
                    }
                },
                NetRequest::Send { id, frame, .. } => {
                    if let Some(c) = self.conns.get(&id).filter(|c| c.state == ConnState::Open) {
                        self.transport.send(c.wire, frame);
                    }
                }
                NetRequest::Close { id, code, reason, .. } => {
                    if let Some(c) = self.conns.get(&id) {
                        self.transport.close(c.wire, code, &reason);
                    }
                }
            }
        }
        log
    }

    /// What to deliver this frame: core's own events, then what the transport received, with
    /// the queue bounds applied. Returns the batch and log lines for crossed bounds.
    pub fn take_batch(&mut self) -> (Vec<NetEvent>, Vec<String>) {
        let mut log = Vec::new();
        self.polled.clear();
        self.transport.poll(&mut self.polled);
        let mut batch = std::mem::take(&mut self.batch);
        batch.clear();
        batch.append(&mut self.pending);
        let mut queued: HashMap<ConnId, (usize, usize)> = HashMap::new();
        let polled = std::mem::take(&mut self.polled);
        for event in polled.iter() {
            let Some(&id) = self.wires.get(&event.id()) else { continue };
            let Some(conn) = self.conns.get_mut(&id) else { continue };
            let event = &with_id(event, id);
            match event {
                NetEvent::Open { .. } => {
                    conn.state = ConnState::Open;
                    batch.push(event.clone());
                }
                NetEvent::Message { frame, .. } => {
                    let q = queued.entry(id).or_default();
                    let bound = if frame.len() > MAX_MESSAGE_BYTES {
                        Some(format!("a message of {} bytes is over the 16 MiB limit", frame.len()))
                    } else if q.0 + 1 > MAX_QUEUED_MESSAGES {
                        Some(format!("more than {MAX_QUEUED_MESSAGES} messages arrived before the game could take them"))
                    } else if q.1 + frame.len() > MAX_QUEUED_BYTES {
                        Some("more than 64 MiB arrived before the game could take it".to_string())
                    } else {
                        None
                    };
                    match bound {
                        None => {
                            q.0 += 1;
                            q.1 += frame.len();
                            batch.push(event.clone());
                        }
                        Some(why) => {
                            log.push(format!("WebSocket to {} closed with 1009: {why}", conn.origin));
                            self.close_quietly(id, TOO_BIG, "message too big");
                            batch.push(NetEvent::Error { id });
                            batch.push(NetEvent::Close { id, code: TOO_BIG, reason: "message too big".into(), clean: false });
                        }
                    }
                }
                NetEvent::Close { code, clean, .. } => {
                    if *code == TOO_BIG && !clean {
                        log.push(format!("WebSocket to {} closed with 1009: a message was over the 16 MiB limit", conn.origin));
                    }
                    self.forget(id);
                    if !clean {
                        batch.push(NetEvent::Error { id });
                    }
                    batch.push(event.clone());
                }
                NetEvent::Error { .. } | NetEvent::Written { .. } => batch.push(event.clone()),
            }
        }
        self.polled = polled;
        self.transport.take_log(&mut log);
        (batch, log)
    }

    /// Hands a batch's buffer back for reuse.
    fn recycle(&mut self, mut batch: Vec<NetEvent>) {
        batch.clear();
        self.batch = batch;
    }
}

/// `event` as scripts know it: under their id, not the transport's.
fn with_id(event: &NetEvent, id: ConnId) -> NetEvent {
    let mut e = event.clone();
    match &mut e {
        NetEvent::Open { id: i, .. }
        | NetEvent::Message { id: i, .. }
        | NetEvent::Error { id: i }
        | NetEvent::Close { id: i, .. }
        | NetEvent::Written { id: i, .. } => *i = id,
    }
    e
}

/// Whether the engine is in edit mode.
fn editing(world: &World) -> bool {
    world.get_resource::<State<crate::EngineMode>>().is_some_and(|s| *s.get() == crate::EngineMode::Edit)
}

fn log_warnings(world: &mut World, lines: Vec<String>) {
    if lines.is_empty() {
        return;
    }
    let generation = current_generation(world);
    let mut log = world.resource_mut::<ScriptLog>();
    for line in lines {
        log.push(LogLevel::Warn, line, None, generation);
    }
}

/// Applies requests scripts made, as the live generation's, logging refusals.
pub fn apply_requests(world: &mut World, requests: Vec<NetRequest>) {
    if requests.is_empty() {
        return;
    }
    let Some(mut net) = world.remove_non_send::<NetRegistry>() else { return };
    let live = current_generation(world);
    let edit = editing(world);
    let policy = world.get_resource::<NetPolicy>().cloned().unwrap_or_default();
    let lines = net.apply(requests, live, &policy, edit);
    world.insert_non_send(net);
    log_warnings(world, lines);
}

/// The milliseconds of real time timers measure.
pub fn now_ms(world: &World) -> f64 {
    world.get_resource::<Time<Real>>().map_or(0.0, |t| t.elapsed().as_secs_f64() * 1000.0)
}

/// `First`, in play, after time advances and a pending generation applies: polls the transport,
/// hands the frame's events and the real time to the backend (which calls into JS only when
/// something is due), and applies the requests scripts made.
pub fn net_deliver(world: &mut World) {
    let Some(mut net) = world.remove_non_send::<NetRegistry>() else { return };
    let Some(mut rt) = world.remove_non_send::<ScriptRuntime>() else {
        world.insert_non_send(net);
        return;
    };
    let (batch, lines) = net.take_batch();
    let now = now_ms(world);
    let result = rt.backend.deliver(now, &batch);
    net.recycle(batch);
    world.insert_non_send(net);
    log_warnings(world, lines);
    match result {
        Ok(delivered) => {
            world.insert_non_send(rt);
            apply_requests(world, delivered.requests);
        }
        Err(e) => {
            rt.log_error(world, "network delivery threw", &e);
            world.insert_non_send(rt);
        }
    }
}

/// `OnEnter(EngineMode::Edit)`: edit mode runs no game code, so it holds no connections.
pub fn close_for_edit(world: &mut World) {
    if let Some(mut net) = world.get_non_send_mut::<NetRegistry>() {
        net.close_for_edit();
    }
}

/// Closes every connection: the engine instance was replaced.
pub fn close_all(world: &mut World) {
    if let Some(mut net) = world.get_non_send_mut::<NetRegistry>() {
        net.close_all();
    }
}

/// Closes connections of every generation but `live`, after `live` was committed.
pub fn close_other_generations(world: &mut World, live: u64) {
    if let Some(mut net) = world.get_non_send_mut::<NetRegistry>() {
        net.close_other_generations(live);
    }
}

/// Replaces the transport with a fake one, for tests, and returns its handle.
pub fn install_fake(app: &mut App) -> FakeTransport {
    let fake = FakeTransport::default();
    app.insert_non_send(NetRegistry::new(Box::new(fake.clone())));
    fake
}
