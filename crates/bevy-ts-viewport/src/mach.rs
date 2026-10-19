//! The viewport's Mach channel, in the framing of abyme's browser host (`ipc/ipc.h`): the engine
//! checks in one bootstrap name per attach; a viewer looks it up and sends `hello` with a reply
//! port. Each message is a JSON string and at most one Mach port. The JSON is a JSON-RPC 2.0
//! notification whose method and params are in `protocol/bevy-ts.openrpc.json`.

use mach2::bootstrap::{bootstrap_check_in, bootstrap_look_up, bootstrap_port};
use mach2::kern_return::{KERN_SUCCESS, kern_return_t};
use mach2::mach_port::{mach_port_allocate, mach_port_deallocate, mach_port_mod_refs};
use mach2::message::*;
use mach2::port::{MACH_PORT_NULL, MACH_PORT_RIGHT_RECEIVE, mach_port_t};
use mach2::traps::mach_task_self;
use serde_json::{Value, json};
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Largest JSON payload, as in `ipc.h`.
pub const MAX: usize = 60000;
/// How long a send waits for room in the receiver's queue.
const SEND_TIMEOUT_MS: u32 = 200;
const MACH_NOTIFY_DEAD_NAME: i32 = 0o110;

unsafe extern "C" {
    fn mach_port_request_notification(
        task: mach_port_t,
        name: mach_port_t,
        msgid: i32,
        sync: u32,
        notify: mach_port_t,
        notify_poly: mach_msg_type_name_t,
        previous: *mut mach_port_t,
    ) -> kern_return_t;
    fn mach_msg_destroy(msg: *mut mach_msg_header_t);
}

#[repr(C)]
struct Msg {
    header: mach_msg_header_t,
    body: mach_msg_body_t,
    port: mach_msg_port_descriptor_t,
    len: u32,
    data: [u8; MAX],
}

#[repr(C)]
struct Rcv {
    m: Msg,
    trailer: [u8; 128],
}

/// One message off the channel.
#[derive(Debug)]
pub enum Incoming {
    /// A viewer said hello; it is now the one the engine sends to.
    Hello,
    /// The viewer's reply port died: it quit or crashed.
    Gone,
    /// Any other notification, with the port it carried (a send right the receiver now owns).
    Message { method: String, params: Value, port: Option<mach_port_t> },
    /// A payload that is not a JSON-RPC notification.
    Malformed(String),
}

fn send_to(dest: mach_port_t, json: &[u8], port: mach_port_t, disposition: mach_msg_type_name_t) -> kern_return_t {
    if dest == MACH_PORT_NULL {
        return MACH_SEND_INVALID_DEST;
    }
    if json.len() > MAX {
        return mach2::kern_return::KERN_INVALID_ARGUMENT;
    }
    let mut m: Box<Msg> = unsafe { Box::new_zeroed().assume_init() };
    let size = ((std::mem::offset_of!(Msg, data) + json.len() + 3) & !3) as u32;
    m.header.msgh_bits = MACH_MSGH_BITS(MACH_MSG_TYPE_COPY_SEND, 0) | MACH_MSGH_BITS_COMPLEX;
    m.header.msgh_remote_port = dest;
    m.header.msgh_size = size;
    m.body.msgh_descriptor_count = 1;
    m.port = mach_msg_port_descriptor_t::new(port, if port == MACH_PORT_NULL { 0 } else { disposition });
    m.len = json.len() as u32;
    m.data[..json.len()].copy_from_slice(json);
    unsafe {
        mach_msg(&mut m.header, MACH_SEND_MSG | MACH_SEND_TIMEOUT, size, 0, MACH_PORT_NULL, SEND_TIMEOUT_MS, MACH_PORT_NULL)
    }
}

/// A notification's JSON.
pub fn notification(method: &str, params: Value) -> String {
    json!({ "jsonrpc": "2.0", "method": method, "params": params }).to_string()
}

fn parse(data: &[u8], port: Option<mach_port_t>) -> Incoming {
    let Ok(v) = serde_json::from_slice::<Value>(data) else {
        return Incoming::Malformed(String::from_utf8_lossy(data).into_owned());
    };
    match v.get("method").and_then(Value::as_str) {
        Some(m) if v.get("id").is_none() => {
            Incoming::Message { method: m.to_string(), params: v.get("params").cloned().unwrap_or(Value::Null), port }
        }
        _ => Incoming::Malformed(v.to_string()),
    }
}

/// Receives one message on `port`; `None` once the port is gone.
fn receive(port: mach_port_t, buf: &mut Rcv) -> Option<Result<(i32, Option<mach_port_t>, usize), kern_return_t>> {
    buf.m.header = mach_msg_header_t::default();
    let kr = unsafe {
        mach_msg(&mut buf.m.header, MACH_RCV_MSG, 0, size_of::<Rcv>() as u32, port, MACH_MSG_TIMEOUT_NONE, MACH_PORT_NULL)
    };
    if kr == MACH_RCV_PORT_DIED || kr == MACH_RCV_INVALID_NAME {
        return None;
    }
    if kr != KERN_SUCCESS {
        return Some(Err(kr));
    }
    let complex = buf.m.header.msgh_bits & MACH_MSGH_BITS_COMPLEX != 0;
    let carried = (complex && buf.m.port.name != MACH_PORT_NULL).then_some(buf.m.port.name);
    let len = if buf.m.len as usize > MAX { 0 } else { buf.m.len as usize };
    Some(Ok((buf.m.header.msgh_id, carried, len)))
}

struct Inner {
    port: mach_port_t,
    viewer: Mutex<mach_port_t>,
    closed: AtomicBool,
}

impl Inner {
    fn drop_viewer(viewer: &mut mach_port_t) {
        if *viewer != MACH_PORT_NULL {
            unsafe { mach_port_deallocate(mach_task_self(), *viewer) };
        }
        *viewer = MACH_PORT_NULL;
    }
}

/// The engine's end: a checked-in bootstrap name and the viewer that said hello on it.
pub struct Service {
    inner: Arc<Inner>,
    pub name: String,
}

impl Service {
    /// Checks in `name` and receives on a thread of its own, calling `on` for each message.
    pub fn open(name: &str, mut on: impl FnMut(Incoming) + Send + 'static) -> Result<Service, String> {
        let c = CString::new(name).map_err(|e| e.to_string())?;
        let mut port = MACH_PORT_NULL;
        let kr = unsafe { bootstrap_check_in(bootstrap_port, c.as_ptr(), &mut port) };
        if kr != KERN_SUCCESS {
            return Err(format!("bootstrap_check_in({name}) failed: {kr}"));
        }
        let inner = Arc::new(Inner { port, viewer: Mutex::new(MACH_PORT_NULL), closed: AtomicBool::new(false) });
        let thread = inner.clone();
        std::thread::Builder::new()
            .name("bevy-ts-viewport-rx".into())
            .spawn(move || {
                let mut buf: Box<Rcv> = unsafe { Box::new_zeroed().assume_init() };
                while let Some(r) = receive(thread.port, &mut buf) {
                    if thread.closed.load(Ordering::Acquire) {
                        break;
                    }
                    let Ok((id, carried, len)) = r else { continue };
                    if id == MACH_NOTIFY_DEAD_NAME {
                        // mach_dead_name_notification_t: header, NDR record (8 bytes), not_port.
                        let dead = unsafe {
                            (&buf.m as *const Msg as *const u8).add(size_of::<mach_msg_header_t>() + 8).cast::<mach_port_t>().read_unaligned()
                        };
                        let mut viewer = thread.viewer.lock().unwrap();
                        let was = dead == *viewer;
                        if was {
                            Inner::drop_viewer(&mut viewer);
                        }
                        drop(viewer);
                        unsafe { mach_port_deallocate(mach_task_self(), dead) };
                        if was {
                            on(Incoming::Gone);
                        }
                        continue;
                    }
                    if id != 0 {
                        unsafe { mach_msg_destroy(&mut buf.m.header) };
                        continue;
                    }
                    match parse(&buf.m.data[..len], carried) {
                        Incoming::Message { method, port: Some(reply), .. } if method == "hello" => {
                            let mut viewer = thread.viewer.lock().unwrap();
                            Inner::drop_viewer(&mut viewer);
                            *viewer = reply;
                            let mut prev = MACH_PORT_NULL;
                            unsafe {
                                mach_port_request_notification(
                                    mach_task_self(),
                                    reply,
                                    MACH_NOTIFY_DEAD_NAME,
                                    0,
                                    thread.port,
                                    MACH_MSG_TYPE_MAKE_SEND_ONCE,
                                    &mut prev,
                                );
                                if prev != MACH_PORT_NULL {
                                    mach_port_deallocate(mach_task_self(), prev);
                                }
                            }
                            drop(viewer);
                            on(Incoming::Hello);
                        }
                        Incoming::Message { method, .. } if method == "bye" => {
                            Inner::drop_viewer(&mut thread.viewer.lock().unwrap());
                            on(Incoming::Gone);
                        }
                        m => on(m),
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Service { inner, name: name.to_string() })
    }

    pub fn attached(&self) -> bool {
        *self.inner.viewer.lock().unwrap() != MACH_PORT_NULL
    }

    /// Forgets the viewer, as if it had said `bye`.
    pub fn detach(&self) {
        Inner::drop_viewer(&mut self.inner.viewer.lock().unwrap());
    }

    fn send_raw(&self, json: &str, port: mach_port_t, disposition: mach_msg_type_name_t) -> Result<(), kern_return_t> {
        let mut viewer = self.inner.viewer.lock().unwrap();
        let kr = send_to(*viewer, json.as_bytes(), port, disposition);
        if kr == MACH_SEND_INVALID_DEST {
            Inner::drop_viewer(&mut viewer);
        }
        if kr == KERN_SUCCESS { Ok(()) } else { Err(kr) }
    }

    /// Sends a notification to the viewer.
    pub fn send(&self, method: &str, params: Value) -> Result<(), kern_return_t> {
        self.send_raw(&notification(method, params), MACH_PORT_NULL, 0)
    }

    /// Sends a notification carrying `port`, a send right the call consumes either way.
    pub fn send_port(&self, method: &str, params: Value, port: mach_port_t) -> Result<(), kern_return_t> {
        let r = self.send_raw(&notification(method, params), port, MACH_MSG_TYPE_MOVE_SEND);
        if r.is_err() {
            unsafe { mach_port_deallocate(mach_task_self(), port) };
        }
        r
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        self.inner.closed.store(true, Ordering::Release);
        Inner::drop_viewer(&mut self.inner.viewer.lock().unwrap());
        // The receive thread wakes with the port dead, which also ends the bootstrap name.
        unsafe { mach_port_mod_refs(mach_task_self(), self.inner.port, MACH_PORT_RIGHT_RECEIVE, -1) };
    }
}

/// A viewer's end, for tests and the throwaway viewer: looks a service up, says hello, and
/// queues what the engine sends.
pub struct Viewer {
    host: mach_port_t,
    reply: mach_port_t,
    rx: Receiver<Incoming>,
}

impl Viewer {
    pub fn connect(name: &str) -> Result<Viewer, String> {
        let c = CString::new(name).map_err(|e| e.to_string())?;
        let mut host = MACH_PORT_NULL;
        let kr = unsafe { bootstrap_look_up(bootstrap_port, c.as_ptr(), &mut host) };
        if kr != KERN_SUCCESS {
            return Err(format!("bootstrap_look_up({name}) failed: {kr}"));
        }
        let mut reply = MACH_PORT_NULL;
        unsafe { mach_port_allocate(mach_task_self(), MACH_PORT_RIGHT_RECEIVE, &mut reply) };
        let (tx, rx): (Sender<Incoming>, _) = channel();
        std::thread::Builder::new()
            .name("bevy-ts-viewer-rx".into())
            .spawn(move || {
                let mut buf: Box<Rcv> = unsafe { Box::new_zeroed().assume_init() };
                while let Some(r) = receive(reply, &mut buf) {
                    let Ok((_, carried, len)) = r else { continue };
                    if tx.send(parse(&buf.m.data[..len], carried)).is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        let hello = notification("hello", json!({}));
        let kr = send_to(host, hello.as_bytes(), reply, MACH_MSG_TYPE_MAKE_SEND);
        if kr != KERN_SUCCESS {
            return Err(format!("hello failed: {kr}"));
        }
        Ok(Viewer { host, reply, rx })
    }

    pub fn send(&self, method: &str, params: Value) -> Result<(), kern_return_t> {
        self.send_json(&notification(method, params))
    }

    /// Sends a payload as is, for tests of malformed messages.
    pub fn send_json(&self, json: &str) -> Result<(), kern_return_t> {
        let kr = send_to(self.host, json.as_bytes(), MACH_PORT_NULL, 0);
        if kr == KERN_SUCCESS { Ok(()) } else { Err(kr) }
    }

    pub fn recv(&self, timeout: Duration) -> Option<Incoming> {
        match self.rx.recv_timeout(timeout) {
            Ok(m) => Some(m),
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => None,
        }
    }
}

impl Drop for Viewer {
    fn drop(&mut self) {
        unsafe {
            mach_port_mod_refs(mach_task_self(), self.reply, MACH_PORT_RIGHT_RECEIVE, -1);
            mach_port_deallocate(mach_task_self(), self.host);
        }
    }
}

/// Releases a send right received in a message.
pub fn release_port(port: mach_port_t) {
    unsafe { mach_port_deallocate(mach_task_self(), port) };
}
