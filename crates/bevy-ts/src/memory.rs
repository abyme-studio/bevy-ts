//! What a system keeps across runs and reloads, keyed by its identity (`module#export`):
//! its last-run tick (for `Changed`/`Added`), its `Local` storage and its message cursors.

use crate::heap::{self, HeapKind, HeapShape};
use crate::layout::ComponentLayout;
use crate::manifest::FieldDecl;
use crate::value;
use bevy::ecs::change_detection::Tick;
use bevy::prelude::*;
use std::collections::HashMap;

/// A `Local<{...}>`: its declaration, layout and bytes. With a heap field (`string`, `T[]`) the
/// bytes own their heap values, as a component's do (`crate::heap`): not `Clone`, dropped through
/// the shape, moved field by field on a redeclare.
#[derive(Debug)]
pub struct LocalStore {
    pub fields: Vec<FieldDecl>,
    pub layout: ComponentLayout,
    /// 16-aligned storage; `layout.size` bytes are used.
    pub bytes: Vec<u128>,
    /// The layout's heap fields, when it has one: the bytes start with its header.
    pub shape: Option<&'static HeapShape>,
}

impl LocalStore {
    fn new(fields: &[FieldDecl]) -> Self {
        let layout = ComponentLayout::c_layout(fields.iter().map(|f| (f.name.as_str(), &f.ty)));
        let shape = HeapShape::of("Local", &layout);
        let mut store = LocalStore { fields: fields.to_vec(), bytes: vec![0; layout.size.div_ceil(16).max(1)], layout, shape };
        let decl = crate::manifest::ComponentDecl { name: "Local".into(), fields: fields.to_vec(), ..Default::default() };
        match value::encode_component(&decl, &store.layout, &serde_json::Map::new()) {
            Ok(init) => {
                store.data_mut()[..init.len()].copy_from_slice(init.bytes());
                // The store owns the heap values now.
                drop(init.into_moved());
            }
            Err(e) => {
                // Defaults were checked at load; a store with no value would have no heap values
                // for its drop to free.
                assert!(store.shape.is_none(), "a Local with heap fields and no default: {e}");
            }
        }
        store
    }

    pub fn data_mut(&mut self) -> &mut [u8] {
        let n = self.layout.size;
        // SAFETY: `bytes` holds at least `n` bytes.
        unsafe { std::slice::from_raw_parts_mut(self.bytes.as_mut_ptr() as *mut u8, n) }
    }

    pub fn data(&self) -> &[u8] {
        // SAFETY: as above.
        unsafe { std::slice::from_raw_parts(self.bytes.as_ptr() as *const u8, self.layout.size) }
    }

    /// Re-declares the store: same layout keeps the bytes, a changed one migrates by field name.
    /// A kept heap field's value moves into the new store; the old one is left empty.
    fn redeclare(&mut self, fields: &[FieldDecl]) {
        let layout = ComponentLayout::c_layout(fields.iter().map(|f| (f.name.as_str(), &f.ty)));
        if layout == self.layout {
            self.fields = fields.to_vec();
            return;
        }
        let mut next = LocalStore::new(fields);
        for f in &next.layout.fields.clone() {
            let Some(old) = self.layout.field(&f.name).filter(|old| old.ty == f.ty).cloned() else { continue };
            match HeapKind::of(&f.ty) {
                Some(kind) => {
                    // SAFETY: both slots hold live values of `kind` (each store's layout says so).
                    unsafe {
                        let v = heap::take_value(self.data_mut().as_mut_ptr(), old.offset, kind);
                        heap::replace(next.data_mut().as_mut_ptr(), f.offset, v);
                    }
                }
                None => {
                    let w = f.ty.size();
                    let src = self.data()[old.offset..old.offset + w].to_vec();
                    next.data_mut()[f.offset..f.offset + w].copy_from_slice(&src);
                }
            }
        }
        *self = next;
    }
}

impl Drop for LocalStore {
    fn drop(&mut self) {
        if let Some(shape) = self.shape {
            // SAFETY: the bytes are a live value of `shape` that owns its heap values, dropped once.
            unsafe { heap::drop_value(self.data_mut().as_mut_ptr(), shape) };
        }
    }
}

#[derive(Debug, Default)]
pub struct Memory {
    pub last_run: Tick,
    pub locals: Vec<LocalStore>,
    /// Message type name to the next sequence number this system has not read.
    pub cursors: HashMap<String, u64>,
}

/// Every system's memory, by identity. Survives reloads; a restart clears it.
#[derive(Resource, Default, Debug)]
pub struct SystemMemory(pub HashMap<String, Memory>);

impl SystemMemory {
    /// The memory of a system, with its `Local`s (re)declared in order.
    pub fn declare(&mut self, identity: &str, locals: &[&[FieldDecl]]) -> &mut Memory {
        let m = self.0.entry(identity.to_string()).or_default();
        m.locals.truncate(locals.len());
        for (i, fields) in locals.iter().enumerate() {
            match m.locals.get_mut(i) {
                Some(store) => store.redeclare(fields),
                None => m.locals.push(LocalStore::new(fields)),
            }
        }
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heap::HeapValue;
    use crate::layout::FieldType;
    use serde_json::json;

    fn field(name: &str, ty: FieldType, default: Option<serde_json::Value>) -> FieldDecl {
        FieldDecl { name: name.into(), ty, default, hints: Default::default() }
    }

    fn shape(fields: &[FieldDecl]) -> &'static HeapShape {
        HeapShape::of("Local", &ComponentLayout::c_layout(fields.iter().map(|f| (f.name.as_str(), &f.ty)))).unwrap()
    }

    fn text(mem: &SystemMemory, name: &str) -> HeapValue {
        let store = &mem.0["it#tally"].locals[0];
        let f = store.layout.field(name).unwrap();
        // SAFETY: the store is a live value of its layout, with a string at `name`.
        unsafe { heap::read(store.data().as_ptr(), f.offset, HeapKind::Str) }
    }

    #[test]
    fn a_locals_string_is_freed_once_on_redeclare_restart_and_drop() {
        // Field types no other test's `Local` has, so these shapes count this test's values only.
        let one = vec![field("note", FieldType::Str, Some(json!("hi"))), field("a", FieldType::F64, None), field("b", FieldType::I16, None)];
        let mut mem = SystemMemory::default();
        mem.declare("it#tally", &[&one]);
        assert_eq!((shape(&one).live(), text(&mem, "note")), (1, HeapValue::Str("hi".into())));

        // A reload with the same layout keeps the store; one that adds a field moves the string.
        mem.declare("it#tally", &[&one]);
        let mut two = one.clone();
        two.push(field("c", FieldType::F64, None));
        mem.declare("it#tally", &[&two]);
        assert_eq!((shape(&one).live(), shape(&two).live(), text(&mem, "note")), (0, 1, HeapValue::Str("hi".into())));

        // A retyped field takes its default; a kept one still moves.
        let mut three = two.clone();
        three[2] = field("b", FieldType::Str, Some(json!("new")));
        mem.declare("it#tally", &[&three]);
        assert_eq!((shape(&two).live(), shape(&three).live()), (0, 1));
        assert_eq!((text(&mem, "note"), text(&mem, "b")), (HeapValue::Str("hi".into()), HeapValue::Str("new".into())));

        // A restart replaces the memory; the world's drop drops it.
        drop(std::mem::take(&mut mem));
        assert_eq!(shape(&three).live(), 0);
        mem.declare("it#tally", &[&three]);
        assert_eq!(shape(&three).live(), 1);
        drop(mem);
        assert_eq!(shape(&three).live(), 0);
    }
}
