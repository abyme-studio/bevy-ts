//! TS messages, events and states, stored in Rust.
//!
//! Messages are double-buffered per type in their native layout and swapped in `First`, so a
//! message lives for two updates, Bevy's rule. Readers keep a sequence-number cursor per system
//! identity. Events are queued for observers (see `observers`). States are small machines whose
//! transitions run in `StateTransition`.
//!
//! A message or event with a heap field (`string`, `T[]`) owns its heap values, as a component
//! does (`crate::heap`): a queue drops each record once, when Bevy's two-frame rule expires it,
//! on a restart and when the world drops; a copy of a queue (a reload's rollback) copies them.

use crate::heap::{HeapBytes, HeapFill, HeapShape};
use crate::layout::ComponentLayout;
use crate::manifest::{ComponentDecl, PayloadDecl, StateDecl};
use bevy::prelude::*;
use std::collections::HashMap;

/// One message type's buffers. With a heap layout each record owns its heap values: dropped once
/// through `fill`'s shape, deep-copied by `clone`.
#[derive(Debug)]
pub struct MessageQueue {
    pub name: String,
    pub layout: ComponentLayout,
    /// The declaration's fields: their defaults, for a write from Rust.
    pub fields: Vec<crate::manifest::FieldDecl>,
    /// How a record with heap fields is built from JS's bytes; `None` for a plain layout.
    pub fill: Option<HeapFill>,
    /// The older half and its first sequence number.
    pub older: Vec<u8>,
    pub older_start: u64,
    /// The newer half and its first sequence number.
    pub newer: Vec<u8>,
    pub newer_start: u64,
    pub next_seq: u64,
}

impl MessageQueue {
    fn new(name: &str, layout: ComponentLayout, fields: Vec<crate::manifest::FieldDecl>, fill: Option<HeapFill>) -> Self {
        MessageQueue { name: name.into(), layout, fields, fill, older: vec![], older_start: 0, newer: vec![], newer_start: 0, next_seq: 0 }
    }

    fn stride(&self) -> usize {
        self.layout.size.max(1)
    }

    fn shape(&self) -> Option<&'static HeapShape> {
        self.fill.as_ref().map(|f| f.shape)
    }

    /// Appends a record; its heap values move into the queue.
    pub fn push(&mut self, value: HeapBytes) {
        debug_assert!(value.shape().map(std::ptr::from_ref) == self.shape().map(std::ptr::from_ref), "a record of {}'s shape", self.name);
        let mut rec = value.into_moved();
        rec.resize(self.stride(), 0);
        self.newer.extend_from_slice(&rec);
        self.next_seq += 1;
    }

    /// A record from JS's bytes: `layout.size` of them, then a heap patch (`HeapFill::build`).
    pub fn build(&self, js: &[u8]) -> Result<HeapBytes, String> {
        build(&self.name, &self.layout, self.fill.as_ref(), js)
    }

    /// Messages with sequence numbers at or after `cursor`, as up to two runs of records, each
    /// with the sequence number of its first.
    pub fn unread(&self, cursor: u64) -> [(&[u8], usize, u64); 2] {
        let s = self.stride();
        let part = |buf: &'_ [u8], start: u64| -> (usize, usize, u64) {
            let count = (buf.len() / s) as u64;
            let from = cursor.max(start).min(start + count) - start;
            (from as usize * s, (count - from) as usize, start + from)
        };
        let (o_off, o_n, o_seq) = part(&self.older, self.older_start);
        let (n_off, n_n, n_seq) = part(&self.newer, self.newer_start);
        [(&self.older[o_off..], o_n, o_seq), (&self.newer[n_off..], n_n, n_seq)]
    }

    /// The record with sequence number `seq`, while it is readable.
    pub fn record(&self, seq: u64) -> Option<*const u8> {
        let s = self.stride();
        [(&self.older, self.older_start), (&self.newer, self.newer_start)].into_iter().find_map(|(buf, start)| {
            let i = seq.checked_sub(start)? as usize;
            (i < buf.len() / s).then(|| buf[i * s..].as_ptr())
        })
    }

    pub fn has_unread(&self, cursor: u64) -> bool {
        cursor < self.next_seq
    }

    /// Bevy's update: the older half is dropped, the newer becomes older.
    pub fn update(&mut self) {
        let mut gone = std::mem::replace(&mut self.older, std::mem::take(&mut self.newer));
        self.drop_records(&mut gone);
        self.older_start = self.newer_start;
        self.newer_start = self.next_seq;
    }

    /// Drops each record's heap values in `buf`, once.
    fn drop_records(&self, buf: &mut [u8]) {
        let Some(shape) = self.shape() else { return };
        for rec in buf.chunks_exact_mut(self.stride()) {
            // SAFETY: each record is a live value of `shape` that owns its heap values, dropped
            // once here; the buffer is cleared or freed after.
            unsafe { crate::heap::drop_value(rec.as_mut_ptr(), shape) };
        }
    }

    /// A copy of each record of `buf`, with its own heap values.
    fn copy_records(&self, buf: &[u8]) -> Vec<u8> {
        let Some(shape) = self.shape() else { return buf.to_vec() };
        let mut out = Vec::with_capacity(buf.len());
        for rec in buf.chunks_exact(self.stride()) {
            // SAFETY: each record is a live value of `shape`.
            let copy = unsafe { HeapBytes::copy_of(rec.as_ptr(), shape) };
            out.extend_from_slice(&copy.into_moved());
        }
        out
    }
}

impl Clone for MessageQueue {
    fn clone(&self) -> Self {
        MessageQueue {
            name: self.name.clone(),
            layout: self.layout.clone(),
            fields: self.fields.clone(),
            fill: self.fill.clone(),
            older: self.copy_records(&self.older),
            older_start: self.older_start,
            newer: self.copy_records(&self.newer),
            newer_start: self.newer_start,
            next_seq: self.next_seq,
        }
    }
}

impl Drop for MessageQueue {
    fn drop(&mut self) {
        let (mut older, mut newer) = (std::mem::take(&mut self.older), std::mem::take(&mut self.newer));
        self.drop_records(&mut older);
        self.drop_records(&mut newer);
    }
}

/// A value of a payload's layout from JS's bytes: plain, or with heap fields through `fill`.
fn build(name: &str, layout: &ComponentLayout, fill: Option<&HeapFill>, js: &[u8]) -> Result<HeapBytes, String> {
    let size = layout.size;
    if js.len() < size {
        return Err(format!("{name}: {} bytes, its layout has {size}", js.len()));
    }
    match fill {
        Some(f) => f.build(&js[..size], &js[size..]),
        None if js.len() == size => Ok(HeapBytes::plain(js.to_vec())),
        None => Err(format!("{name}: {} bytes, its layout has {size}", js.len())),
    }
}

/// How a payload's heap fields are built, from its declaration.
fn fill_of(p: &PayloadDecl, layout: &ComponentLayout) -> Option<HeapFill> {
    let decl = ComponentDecl { name: p.name.clone(), fields: p.fields.clone(), ..Default::default() };
    HeapFill::of(&decl, layout)
}

/// An event type for observers.
#[derive(Clone, Debug)]
pub struct EventType {
    pub name: String,
    pub layout: ComponentLayout,
    /// How a value with heap fields is built from JS's bytes; `None` for a plain layout.
    pub fill: Option<HeapFill>,
    pub entity: bool,
    pub propagate: bool,
}

impl EventType {
    /// A value from JS's bytes: `layout.size` of them, then a heap patch (`HeapFill::build`).
    pub fn build(&self, js: &[u8]) -> Result<HeapBytes, String> {
        build(&self.name, &self.layout, self.fill.as_ref(), js)
    }
}

/// A TS state machine: its values (by name and symbol) and its slot. Its value is Bevy's
/// `State<TsState<N>>` (crate::states).
#[derive(Clone, Debug)]
pub struct StateMachine {
    pub name: String,
    pub values: Vec<String>,
    /// Each value's interned symbol, in `values` order.
    pub syms: Vec<u32>,
    pub slot: u8,
    /// The `initState` value's symbol.
    pub initial: Option<u32>,
    /// A sub state's source: the machine's index here and the value's symbol.
    pub source: Option<(usize, u32)>,
    /// A computed state's sources, by index here; empty for others.
    pub computed: Vec<usize>,
}

impl StateMachine {
    /// A sub or computed state: its value follows its sources.
    pub fn derived(&self) -> bool {
        self.source.is_some() || !self.computed.is_empty()
    }
}

impl StateMachine {
    pub fn index(&self, value: &str) -> Option<u32> {
        self.values.iter().position(|v| v == value).map(|i| i as u32)
    }

    /// A value's symbol by name.
    pub fn sym(&self, value: &str) -> Option<u32> {
        self.index(value).map(|i| self.syms[i as usize])
    }

    /// The index in `values` of a symbol, as JS numbers values.
    pub fn index_of_sym(&self, sym: u32) -> Option<u32> {
        self.syms.iter().position(|s| *s == sym).map(|i| i as u32)
    }
}

/// Every TS message type, event type and state machine, by index (the index ops carry). A clone
/// copies each queued message's heap values (`MessageQueue::clone`).
#[derive(Resource, Default, Debug, Clone)]
pub struct TsPayloads {
    pub messages: Vec<MessageQueue>,
    pub events: Vec<EventType>,
    pub states: Vec<StateMachine>,
}

impl TsPayloads {
    pub fn message(&self, name: &str) -> Option<usize> {
        self.messages.iter().position(|m| m.name == name)
    }
    pub fn event(&self, name: &str) -> Option<usize> {
        self.events.iter().position(|e| e.name == name)
    }
    pub fn state(&self, name: &str) -> Option<usize> {
        self.states.iter().position(|s| s.name == name)
    }

    /// Applies a generation's declarations. Message queues and state values persist by name;
    /// a message whose layout changed starts empty.
    /// `slot` gives each machine's slot (crate::states::TsStates::assign).
    pub fn declare(&mut self, messages: &[PayloadDecl], events: &[PayloadDecl], states: &[StateDecl], slot: &mut dyn FnMut(&str) -> u8) {
        for m in messages {
            let layout = m.layout();
            let fill = fill_of(m, &layout);
            match self.message(&m.name) {
                Some(i) if self.messages[i].layout == layout => (self.messages[i].fields, self.messages[i].fill) = (m.fields.clone(), fill),
                Some(i) => self.messages[i] = MessageQueue::new(&m.name, layout, m.fields.clone(), fill),
                None => self.messages.push(MessageQueue::new(&m.name, layout, m.fields.clone(), fill)),
            }
        }
        for e in events {
            let layout = e.layout();
            let t = EventType { name: e.name.clone(), fill: fill_of(e, &layout), layout, entity: e.entity, propagate: e.propagate };
            match self.event(&e.name) {
                Some(i) => self.events[i] = t,
                None => self.events.push(t),
            }
        }
        for st in states {
            let machine = StateMachine {
                name: st.name.clone(),
                values: st.values.clone(),
                syms: st.values.iter().map(|v| crate::states::sym(v)).collect(),
                slot: slot(&st.name),
                initial: st.initial.as_deref().map(crate::states::sym),
                source: None,
                computed: Vec::new(),
            };
            match self.state(&st.name) {
                Some(i) => self.states[i] = machine,
                None => self.states.push(machine),
            }
        }
        // Sources by index, once every machine of the generation is in.
        for st in states {
            let i = self.state(&st.name).expect("declared above");
            let source = st.source.as_ref().and_then(|s| self.state(&s.state).map(|p| (p, crate::states::sym(&s.value))));
            let computed = st.computed.iter().flatten().filter_map(|n| self.state(n)).collect();
            let m = &mut self.states[i];
            m.source = source;
            m.computed = computed;
        }
    }

    /// Rust states TS names (crate::states::RustStates), after the generation's own: each
    /// `(type path, values)` takes slot `SLOTS + n`.
    pub fn declare_rust(&mut self, states: &[(String, Vec<String>)]) {
        for (n, (path, values)) in states.iter().enumerate() {
            let machine = StateMachine {
                name: path.clone(),
                values: values.clone(),
                syms: values.iter().map(|v| crate::states::sym(v)).collect(),
                slot: (crate::states::SLOTS + n) as u8,
                initial: None,
                source: None,
                computed: Vec::new(),
            };
            match self.state(path) {
                Some(i) => self.states[i] = machine,
                None => self.states.push(machine),
            }
        }
    }

    /// Forgets every message, as a restart does (states lose their values in crate::states).
    pub fn reset(&mut self) {
        self.messages.iter_mut().for_each(|m| *m = MessageQueue::new(&m.name.clone(), m.layout.clone(), m.fields.clone(), m.fill.clone()));
    }
}

/// `First`: Bevy's two-frame message rule.
pub fn update_messages(mut payloads: ResMut<TsPayloads>) {
    payloads.messages.iter_mut().for_each(MessageQueue::update);
}

/// Values by machine, for the protocol and tests.
pub fn state_values(world: &World) -> HashMap<String, Option<String>> {
    world
        .resource::<TsPayloads>()
        .states
        .iter()
        .map(|s| (s.name.clone(), crate::states::current_of(world, s.slot).map(|v| crate::states::name(v).to_string())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::FieldType;

    #[test]
    fn messages_live_two_updates_and_readers_keep_cursors() {
        let layout = ComponentLayout::c_layout([("amount", &FieldType::F32)]);
        let mut q = MessageQueue::new("game::Damage", layout, vec![], None);
        let rec = |v: f32| HeapBytes::plain(v.to_le_bytes().to_vec());
        q.push(rec(1.0));
        q.push(rec(2.0));
        let [a, b] = q.unread(0);
        assert_eq!(a.1 + b.1, 2);
        q.update();
        q.push(rec(3.0));
        let [a, b] = q.unread(2);
        assert_eq!((a.1, b.1, b.2), (0, 1, 2));
        assert_eq!(q.unread(0)[0].1, 2, "still readable one update later");
        q.update();
        assert_eq!(q.unread(0)[0].1, 1, "the first two are gone after two updates");
    }

    #[test]
    fn a_heap_queue_frees_each_record_once_on_update_copy_and_drop() {
        use crate::heap::HeapValue;
        // Field types no other test's message has, so this shape counts this test's records only.
        let fields = vec![
            crate::manifest::FieldDecl { name: "text".into(), ty: FieldType::Str, default: Some(serde_json::json!("hi")), hints: Default::default() },
            crate::manifest::FieldDecl { name: "w".into(), ty: FieldType::I16, default: None, hints: Default::default() },
            crate::manifest::FieldDecl { name: "z".into(), ty: FieldType::F64, default: None, hints: Default::default() },
        ];
        let decl = PayloadDecl { name: "it::Chat".into(), fields, entity: false, propagate: false, added: true, location: None };
        let layout = decl.layout();
        let mut q = MessageQueue::new(&decl.name, layout.clone(), decl.fields.clone(), fill_of(&decl, &layout));
        let shape = q.shape().expect("a heap layout");
        let js = vec![0u8; layout.size];
        q.push(q.build(&js).unwrap());
        q.push(q.build(&js).unwrap());
        assert_eq!(shape.live(), 2);
        let text = layout.field("text").unwrap().offset;
        // SAFETY: the record is live, with a string at `text`.
        let read = |q: &MessageQueue, seq| unsafe { crate::heap::read(q.record(seq).unwrap(), text, crate::heap::HeapKind::Str) };
        assert_eq!(read(&q, 1), HeapValue::Str("hi".into()));

        let copy = q.clone();
        assert_eq!(shape.live(), 4);
        drop(copy);
        assert_eq!(shape.live(), 2);

        q.update();
        q.push(q.build(&js).unwrap());
        assert_eq!(shape.live(), 3, "the first two are still readable one update later");
        q.update();
        assert_eq!((shape.live(), q.record(0), q.record(2).is_some()), (1, None, true), "dropped after two updates");
        drop(q);
        assert_eq!(shape.live(), 0);
    }
}
