//! What a system keeps across runs and reloads, keyed by its identity (`module#export`):
//! its last-run tick (for `Changed`/`Added`), its `Local` storage and its message cursors.

use crate::layout::ComponentLayout;
use crate::manifest::FieldDecl;
use crate::value;
use bevy::ecs::change_detection::Tick;
use bevy::prelude::*;
use std::collections::HashMap;

/// A `Local<{...}>`: its declaration, layout and bytes.
#[derive(Clone, Debug)]
pub struct LocalStore {
    pub fields: Vec<FieldDecl>,
    pub layout: ComponentLayout,
    /// 16-aligned storage; `layout.size` bytes are used.
    pub bytes: Vec<u128>,
}

impl LocalStore {
    fn new(fields: &[FieldDecl]) -> Self {
        let layout = ComponentLayout::c_layout(fields.iter().map(|f| (f.name.as_str(), &f.ty)));
        let mut store = LocalStore { fields: fields.to_vec(), bytes: vec![0; layout.size.div_ceil(16).max(1)], layout };
        let decl = crate::manifest::ComponentDecl { name: "Local".into(), fields: fields.to_vec(), location: None };
        if let Ok(init) = value::encode_component(&decl, &store.layout, &serde_json::Map::new()) {
            store.data_mut()[..init.len()].copy_from_slice(&init);
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
    fn redeclare(&mut self, fields: &[FieldDecl]) {
        let layout = ComponentLayout::c_layout(fields.iter().map(|f| (f.name.as_str(), &f.ty)));
        if layout == self.layout {
            self.fields = fields.to_vec();
            return;
        }
        let mut next = LocalStore::new(fields);
        for f in &next.layout.fields.clone() {
            if let Some(old) = self.layout.field(&f.name)
                && old.ty == f.ty
            {
                let w = f.ty.size();
                let src = self.data()[old.offset..old.offset + w].to_vec();
                next.data_mut()[f.offset..f.offset + w].copy_from_slice(&src);
            }
        }
        *self = next;
    }
}

#[derive(Clone, Debug, Default)]
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
