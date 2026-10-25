//! TS messages, events and states, stored in Rust.
//!
//! Messages are double-buffered per type in their native layout and swapped in `First`, so a
//! message lives for two updates, Bevy's rule. Readers keep a sequence-number cursor per system
//! identity. Events are queued for observers (see `observers`). States are small machines whose
//! transitions run in `StateTransition`.

use crate::layout::ComponentLayout;
use crate::manifest::{PayloadDecl, StateDecl};
use bevy::prelude::*;
use std::collections::HashMap;

/// One message type's buffers.
#[derive(Clone, Debug)]
pub struct MessageQueue {
    pub name: String,
    pub layout: ComponentLayout,
    /// The older half and its first sequence number.
    pub older: Vec<u8>,
    pub older_start: u64,
    /// The newer half and its first sequence number.
    pub newer: Vec<u8>,
    pub newer_start: u64,
    pub next_seq: u64,
}

impl MessageQueue {
    fn new(name: &str, layout: ComponentLayout) -> Self {
        MessageQueue { name: name.into(), layout, older: vec![], older_start: 0, newer: vec![], newer_start: 0, next_seq: 0 }
    }

    fn stride(&self) -> usize {
        self.layout.size.max(1)
    }

    pub fn push(&mut self, bytes: &[u8]) {
        let mut rec = bytes.to_vec();
        rec.resize(self.stride(), 0);
        self.newer.extend_from_slice(&rec);
        self.next_seq += 1;
    }

    /// Messages with sequence numbers at or after `cursor`, as up to two runs of records.
    pub fn unread(&self, cursor: u64) -> [(&[u8], usize); 2] {
        let s = self.stride();
        let part = |buf: &'_ [u8], start: u64| -> (usize, usize) {
            let count = (buf.len() / s) as u64;
            let from = cursor.max(start).min(start + count) - start;
            (from as usize * s, (count - from) as usize)
        };
        let (o_off, o_n) = part(&self.older, self.older_start);
        let (n_off, n_n) = part(&self.newer, self.newer_start);
        [(&self.older[o_off..], o_n), (&self.newer[n_off..], n_n)]
    }

    pub fn has_unread(&self, cursor: u64) -> bool {
        cursor < self.next_seq
    }

    /// Bevy's update: the older half is dropped, the newer becomes older.
    pub fn update(&mut self) {
        self.older = std::mem::take(&mut self.newer);
        self.older_start = self.newer_start;
        self.newer_start = self.next_seq;
    }
}

/// An event type for observers.
#[derive(Clone, Debug)]
pub struct EventType {
    pub name: String,
    pub layout: ComponentLayout,
    pub entity: bool,
    pub propagate: bool,
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

/// Every TS message type, event type and state machine, by index (the index ops carry).
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
            match self.message(&m.name) {
                Some(i) if self.messages[i].layout == layout => {}
                Some(i) => self.messages[i] = MessageQueue::new(&m.name, layout),
                None => self.messages.push(MessageQueue::new(&m.name, layout)),
            }
        }
        for e in events {
            let t = EventType { name: e.name.clone(), layout: e.layout(), entity: e.entity, propagate: e.propagate };
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
        self.messages.iter_mut().for_each(|m| *m = MessageQueue::new(&m.name.clone(), m.layout.clone()));
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
        let mut q = MessageQueue::new("game::Damage", layout);
        q.push(&1f32.to_le_bytes());
        q.push(&2f32.to_le_bytes());
        let [a, b] = q.unread(0);
        assert_eq!(a.1 + b.1, 2);
        q.update();
        q.push(&3f32.to_le_bytes());
        let [a, b] = q.unread(2);
        assert_eq!((a.1, b.1), (0, 1));
        assert_eq!(q.unread(0)[0].1, 2, "still readable one update later");
        q.update();
        assert_eq!(q.unread(0)[0].1, 1, "the first two are gone after two updates");
    }
}
