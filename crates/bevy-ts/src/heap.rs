//! Heap fields of TS components (`label: string`, `scores: u32[]`): Rust values in the component's
//! own bytes, as a Bevy component holds a `String` or a `Vec<T>`.
//!
//! A component with a heap field starts with [`HEADER`] bytes holding a `&'static HeapShape`: each
//! heap field's offset and kind. The drop fn and the clone fn it registers with read the header,
//! so they need no registry and still work once its class is gone. A component with no heap field
//! has no header, no drop fn and copies its bytes, as before.
//!
//! [`HeapBytes`] is a value's bytes that own their heap values: built by `value.rs` or a
//! [`HeapFill`], moved into a column once by `commands::insert_owned`, dropped through the shape
//! otherwise. Its buffer is unaligned, so every access here reads and writes unaligned.
//!
//! A list of fixed-size items other than numbers (vectors, `Entity`, enums, objects of those) is a
//! raw list ([`ItemShape`]): the three words of a Rust `Vec<T>` of a `T` of the item's layout,
//! built, cloned and freed only here. A Rust system reads it as that `Vec<T>` (a `Vec4` or `Quat`
//! item as `[f32; 4]`, an object as a `#[repr(C)]` struct of its members).

use crate::layout::{ComponentLayout, FieldType};
use bevy::prelude::Entity;
use crate::manifest::ComponentDecl;
use bevy::ecs::entity::{ComponentCloneCtx, SourceComponent};
use bevy::ptr::{OwningPtr, Ptr};
use std::collections::HashMap;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{LazyLock, Mutex};

/// Bytes before the first field of a component with a heap field: the `&'static HeapShape`.
pub const HEADER: usize = 8;

/// A heap slot's bytes: a `String` or a `Vec<T>`.
const HEAP_SLOT: usize = 24;

/// A list's item type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Item {
    F32,
    F64,
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    Bool,
    Str,
    /// A vector, `Entity`, an enum or an object: a raw list of the item's bytes.
    Plain(&'static ItemShape),
}

/// A raw list's item: its type and layout. One per distinct type, leaked once.
#[derive(Debug)]
pub struct ItemShape {
    pub ty: FieldType,
    /// The item's size, a multiple of `align`: the list's stride.
    pub size: usize,
    pub align: usize,
    /// Lists of it in a slot now: for tests, which check every path frees what it built.
    live: AtomicIsize,
}

impl PartialEq for ItemShape {
    fn eq(&self, other: &Self) -> bool {
        self.ty == other.ty
    }
}

impl Eq for ItemShape {}

impl std::hash::Hash for ItemShape {
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
        self.ty.hash(h)
    }
}

impl ItemShape {
    pub fn of(ty: &FieldType) -> &'static ItemShape {
        static SHAPES: LazyLock<Mutex<HashMap<FieldType, &'static ItemShape>>> = LazyLock::new(Default::default);
        let mut shapes = SHAPES.lock().unwrap_or_else(|e| e.into_inner());
        shapes.entry(ty.clone()).or_insert_with(|| {
            let (size, align) = (ty.size(), ty.align());
            assert!(size > 0 && size.is_multiple_of(align), "a list item's size is a multiple of its alignment");
            Box::leak(Box::new(ItemShape { ty: ty.clone(), size, align, live: AtomicIsize::new(0) }))
        })
    }

    /// Lists of this item alive now.
    pub fn live(&self) -> isize {
        self.live.load(Ordering::SeqCst)
    }

    fn layout(&self, cap: usize) -> std::alloc::Layout {
        std::alloc::Layout::from_size_align(self.size * cap, self.align).expect("a list's buffer fits in memory")
    }

    /// One item's bytes in the codec's wire format: numbers and lanes as themselves, an entity as
    /// its bits, an enum as its `u32` variant index, an object as its members in order.
    fn put(ty: &FieldType, b: &[u8], out: &mut Vec<u8>) {
        match ty {
            FieldType::Enum { .. } => {
                let mut raw = [0u8; 4];
                raw[..b.len()].copy_from_slice(b);
                let stored = u32::from_le_bytes(raw);
                out.extend_from_slice(&ty.enum_index(stored).map_or(u32::MAX, |i| i as u32).to_le_bytes());
            }
            FieldType::Struct { .. } => {
                for f in ty.struct_layout().unwrap().fields {
                    ItemShape::put(&f.ty, &b[f.offset..f.offset + f.ty.size()], out);
                }
            }
            _ => out.extend_from_slice(b),
        }
    }

    /// Reads one item of `ty` in the wire format into `out` (`ty.size()` bytes, padding zero).
    fn read(ty: &FieldType, b: &mut &[u8], out: &mut [u8]) -> Result<(), String> {
        match ty {
            FieldType::Enum { variants, .. } => {
                let i = u32::from_le_bytes(take(b, 4)?.try_into().unwrap());
                let stored = ty.enum_value(i as usize).ok_or_else(|| format!("variant {i} of {}", variants.len()))?;
                out.copy_from_slice(&stored.to_le_bytes()[..out.len()]);
            }
            FieldType::Struct { .. } => {
                for f in ty.struct_layout().unwrap().fields {
                    ItemShape::read(&f.ty, b, &mut out[f.offset..f.offset + f.ty.size()])?;
                }
            }
            FieldType::Entity => {
                let bits = u64::from_le_bytes(take(b, 8)?.try_into().unwrap());
                Entity::try_from_bits(bits).ok_or_else(|| format!("{bits} is not an entity"))?;
                out.copy_from_slice(&bits.to_le_bytes());
            }
            FieldType::Bool => out[0] = (take(b, 1)?[0] != 0) as u8,
            _ => out.copy_from_slice(take(b, out.len())?),
        }
        Ok(())
    }

    /// One item's bytes with its padding zeroed, so equal items have equal bytes.
    fn copy_item(ty: &FieldType, src: &[u8], out: &mut [u8]) {
        match ty.struct_layout() {
            Some(l) => l.fields.iter().for_each(|f| out[f.offset..f.offset + f.ty.size()].copy_from_slice(&src[f.offset..f.offset + f.ty.size()])),
            None => out.copy_from_slice(src),
        }
    }
}

/// A raw list's items, owned: `len` items of `shape`, `shape.size` bytes each, padding zero.
#[derive(Clone, Debug, PartialEq)]
pub struct PlainList {
    pub shape: &'static ItemShape,
    pub bytes: Vec<u8>,
}

impl PlainList {
    pub fn len(&self) -> usize {
        self.bytes.len() / self.shape.size
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Item `i`'s bytes.
    pub fn item(&self, i: usize) -> &[u8] {
        &self.bytes[i * self.shape.size..(i + 1) * self.shape.size]
    }
}

/// Where `Vec` keeps its pointer, capacity and length among its three words: not stable Rust, so
/// read once from a real one. A raw list writes its words in that order.
pub fn vec_order() -> [usize; 3] {
    static ORDER: LazyLock<[usize; 3]> = LazyLock::new(|| {
        const _: () = assert!(size_of::<Vec<u8>>() == 3 * size_of::<usize>());
        let mut v: Vec<u8> = Vec::with_capacity(7);
        v.extend_from_slice(&[1, 2]);
        let (ptr, cap) = (v.as_ptr() as usize, v.capacity());
        assert!(cap != 2 && ptr != 2 && ptr != cap);
        // SAFETY: a `Vec` is three words; read as a copy, the `Vec` still owns its buffer.
        let words: [usize; 3] = unsafe { std::mem::transmute_copy(&v) };
        let at = |x: usize| words.iter().position(|w| *w == x).expect("a Vec is a pointer, a capacity and a length");
        [at(ptr), at(cap), at(2)]
    });
    *ORDER
}

/// A raw list's words: its buffer, capacity and length, in items.
#[derive(Clone, Copy)]
struct Raw {
    ptr: *mut u8,
    cap: usize,
    len: usize,
}

impl Raw {
    /// An empty list, as `Vec::new()` makes it: a dangling pointer aligned to the item.
    fn empty(shape: &ItemShape) -> Raw {
        Raw { ptr: std::ptr::without_provenance_mut(shape.align), cap: 0, len: 0 }
    }

    /// A list owning a copy of `items` (`shape.size` bytes each).
    fn of(shape: &ItemShape, items: &[u8]) -> Raw {
        let len = items.len() / shape.size;
        if len == 0 {
            return Raw::empty(shape);
        }
        let layout = shape.layout(len);
        // SAFETY: `layout` has a non-zero size; the buffer is filled before it is read.
        let ptr = unsafe { std::alloc::alloc(layout) };
        if ptr.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        // SAFETY: the buffer holds `items.len()` bytes.
        unsafe { std::ptr::copy_nonoverlapping(items.as_ptr(), ptr, items.len()) };
        Raw { ptr, cap: len, len }
    }

    /// # Safety
    /// `p` holds a raw list's words.
    unsafe fn get(p: *const u8) -> Raw {
        let o = vec_order();
        // SAFETY: the caller's contract.
        let w = |i: usize| unsafe { std::ptr::read_unaligned((p as *const usize).add(o[i])) };
        Raw { ptr: std::ptr::with_exposed_provenance_mut(w(0)), cap: w(1), len: w(2) }
    }

    /// # Safety
    /// `p` has room for a raw list's words; what was there is not dropped. The slot then owns a
    /// list of `shape`.
    unsafe fn put(self, p: *mut u8, shape: &ItemShape) {
        shape.live.fetch_add(1, Ordering::SeqCst);
        let o = vec_order();
        // SAFETY: the caller's contract.
        unsafe {
            std::ptr::write_unaligned((p as *mut usize).add(o[0]), self.ptr.expose_provenance());
            std::ptr::write_unaligned((p as *mut usize).add(o[1]), self.cap);
            std::ptr::write_unaligned((p as *mut usize).add(o[2]), self.len);
        }
    }

    /// The items' bytes.
    ///
    /// # Safety
    /// `self` is a live list of `shape`.
    unsafe fn items<'a>(self, shape: &ItemShape) -> &'a [u8] {
        // SAFETY: the caller's contract: `len` items are initialised.
        unsafe { std::slice::from_raw_parts(self.ptr, self.len * shape.size) }
    }

    /// The items, owned, padding zeroed.
    ///
    /// # Safety
    /// As [`Raw::items`].
    unsafe fn to_list(self, shape: &'static ItemShape) -> PlainList {
        // SAFETY: the caller's contract.
        let src = unsafe { self.items(shape) };
        let mut bytes = vec![0u8; src.len()];
        for (a, b) in src.chunks_exact(shape.size).zip(bytes.chunks_exact_mut(shape.size)) {
            ItemShape::copy_item(&shape.ty, a, b);
        }
        PlainList { shape, bytes }
    }

    /// Frees the buffer of the list a slot held.
    ///
    /// # Safety
    /// `self` is a live list of `shape`, freed once, here.
    unsafe fn free(self, shape: &ItemShape) {
        shape.live.fetch_sub(1, Ordering::SeqCst);
        if self.cap != 0 {
            // SAFETY: allocated with this layout, by `Raw::of` or by a Rust `Vec<T>` of the same.
            unsafe { std::alloc::dealloc(self.ptr, shape.layout(self.cap)) };
        }
    }
}

/// What a heap field holds: a `String`, or a `Vec` of an item type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HeapKind {
    Str,
    List(Item),
}

impl HeapKind {
    pub fn of(ty: &FieldType) -> Option<HeapKind> {
        let item = |t: &FieldType| match t {
            FieldType::F32 => Some(Item::F32),
            FieldType::F64 => Some(Item::F64),
            FieldType::I8 => Some(Item::I8),
            FieldType::U8 => Some(Item::U8),
            FieldType::I16 => Some(Item::I16),
            FieldType::U16 => Some(Item::U16),
            FieldType::I32 => Some(Item::I32),
            FieldType::U32 => Some(Item::U32),
            FieldType::Bool => Some(Item::Bool),
            FieldType::Str => Some(Item::Str),
            t if t.item_member() || matches!(t, FieldType::Struct { .. }) => Some(Item::Plain(ItemShape::of(t))),
            _ => None,
        };
        match ty {
            FieldType::Str => Some(HeapKind::Str),
            FieldType::List { of } => item(of).map(HeapKind::List),
            _ => None,
        }
    }
}

/// A list's items, owned: a `Vec` of its item type.
#[derive(Clone, Debug, PartialEq)]
pub enum List {
    F32(Vec<f32>),
    F64(Vec<f64>),
    I8(Vec<i8>),
    U8(Vec<u8>),
    I16(Vec<i16>),
    U16(Vec<u16>),
    I32(Vec<i32>),
    U32(Vec<u32>),
    Bool(Vec<bool>),
    Str(Vec<String>),
    Plain(PlainList),
}

/// A heap field's value, owned.
#[derive(Clone, Debug, PartialEq)]
pub enum HeapValue {
    Str(String),
    List(List),
}

/// Runs `$body` with `$T` the Rust type a slot of `$kind` holds.
macro_rules! slot_type {
    ($kind:expr, $T:ident => $body:expr) => {
        match $kind {
            HeapKind::Str => {
                type $T = String;
                $body
            }
            HeapKind::List(Item::F32) => {
                type $T = Vec<f32>;
                $body
            }
            HeapKind::List(Item::F64) => {
                type $T = Vec<f64>;
                $body
            }
            HeapKind::List(Item::I8) => {
                type $T = Vec<i8>;
                $body
            }
            HeapKind::List(Item::U8) => {
                type $T = Vec<u8>;
                $body
            }
            HeapKind::List(Item::I16) => {
                type $T = Vec<i16>;
                $body
            }
            HeapKind::List(Item::U16) => {
                type $T = Vec<u16>;
                $body
            }
            HeapKind::List(Item::I32) => {
                type $T = Vec<i32>;
                $body
            }
            HeapKind::List(Item::U32) => {
                type $T = Vec<u32>;
                $body
            }
            HeapKind::List(Item::Bool) => {
                type $T = Vec<bool>;
                $body
            }
            HeapKind::List(Item::Str) => {
                type $T = Vec<String>;
                $body
            }
            HeapKind::List(Item::Plain(_)) => unreachable!("a raw list has no Rust type here"),
        }
    };
}

/// A Rust type a heap slot holds, and its codec form.
trait Slot: Default + Clone + PartialEq + Sized {
    fn into_value(self) -> HeapValue;
    fn from_value(v: HeapValue) -> Self;
    fn put_codec(&self, out: &mut Vec<u8>);
    fn read_codec(b: &mut &[u8]) -> Result<Self, String>;
}

/// One item of a list, in the codec's wire format: a number in its own width, `bool` as a byte,
/// a string as a `u32` byte length then UTF-8.
trait CodecItem: Sized {
    fn put(&self, out: &mut Vec<u8>);
    fn read(b: &mut &[u8]) -> Result<Self, String>;
}

macro_rules! number_items {
    ($($t:ty),*) => {$(
        impl CodecItem for $t {
            fn put(&self, out: &mut Vec<u8>) {
                out.extend_from_slice(&self.to_le_bytes());
            }
            fn read(b: &mut &[u8]) -> Result<Self, String> {
                Ok(<$t>::from_le_bytes(take(b, size_of::<$t>())?.try_into().unwrap()))
            }
        }
    )*};
}
number_items!(f32, f64, i8, u8, i16, u16, i32, u32);

impl CodecItem for bool {
    fn put(&self, out: &mut Vec<u8>) {
        out.push(*self as u8);
    }
    fn read(b: &mut &[u8]) -> Result<Self, String> {
        Ok(take(b, 1)?[0] != 0)
    }
}

impl CodecItem for String {
    fn put(&self, out: &mut Vec<u8>) {
        put_str(self, out);
    }
    fn read(b: &mut &[u8]) -> Result<Self, String> {
        let n = u32::from_le_bytes(take(b, 4)?.try_into().unwrap()) as usize;
        std::str::from_utf8(take(b, n)?).map(str::to_string).map_err(|e| format!("not UTF-8: {e}"))
    }
}

impl Slot for String {
    fn into_value(self) -> HeapValue {
        HeapValue::Str(self)
    }
    fn from_value(v: HeapValue) -> Self {
        match v {
            HeapValue::Str(s) => s,
            other => panic!("a string slot given {other:?}"),
        }
    }
    fn put_codec(&self, out: &mut Vec<u8>) {
        put_str(self, out);
    }
    fn read_codec(b: &mut &[u8]) -> Result<Self, String> {
        <String as CodecItem>::read(b)
    }
}

macro_rules! list_slots {
    ($($v:ident $t:ty),*) => {$(
        impl Slot for Vec<$t> {
            fn into_value(self) -> HeapValue {
                HeapValue::List(List::$v(self))
            }
            fn from_value(v: HeapValue) -> Self {
                match v {
                    HeapValue::List(List::$v(x)) => x,
                    other => panic!("a {} list slot given {other:?}", stringify!($t)),
                }
            }
            fn put_codec(&self, out: &mut Vec<u8>) {
                out.extend_from_slice(&(self.len() as u32).to_le_bytes());
                self.iter().for_each(|x| x.put(out));
            }
            fn read_codec(b: &mut &[u8]) -> Result<Self, String> {
                let n = u32::from_le_bytes(take(b, 4)?.try_into().unwrap()) as usize;
                // Each item is at least a byte: a count past the bytes left is cut short.
                if n > b.len() {
                    return Err("a heap value is cut short".into());
                }
                (0..n).map(|_| <$t as CodecItem>::read(b)).collect()
            }
        }
    )*};
}
list_slots!(F32 f32, F64 f64, I8 i8, U8 u8, I16 i16, U16 u16, I32 i32, U32 u32, Bool bool, Str String);

impl HeapValue {
    /// The empty value of a kind: `""` or `[]`. It allocates nothing.
    pub fn empty(kind: HeapKind) -> HeapValue {
        if let HeapKind::List(Item::Plain(shape)) = kind {
            return HeapValue::List(List::Plain(PlainList { shape, bytes: Vec::new() }));
        }
        slot_type!(kind, T => T::default().into_value())
    }

    /// The kind of slot that holds it.
    pub fn kind(&self) -> HeapKind {
        match self {
            HeapValue::Str(_) => HeapKind::Str,
            HeapValue::List(l) => HeapKind::List(match l {
                List::F32(_) => Item::F32,
                List::F64(_) => Item::F64,
                List::I8(_) => Item::I8,
                List::U8(_) => Item::U8,
                List::I16(_) => Item::I16,
                List::U16(_) => Item::U16,
                List::I32(_) => Item::I32,
                List::U32(_) => Item::U32,
                List::Bool(_) => Item::Bool,
                List::Str(_) => Item::Str,
                List::Plain(l) => Item::Plain(l.shape),
            }),
        }
    }

    /// Reads a value of `kind` in the reflect codec's wire format (`reflect_codec`'s module docs:
    /// a string is a `u32` byte length then UTF-8, a list a `u32` count then its items), advancing
    /// `b` past it.
    pub fn read_codec(kind: HeapKind, b: &mut &[u8]) -> Result<HeapValue, String> {
        if let HeapKind::List(Item::Plain(shape)) = kind {
            let n = u32::from_le_bytes(take(b, 4)?.try_into().unwrap()) as usize;
            // Each item is at least a byte: a count past the bytes left is cut short.
            if n > b.len() {
                return Err("a heap value is cut short".into());
            }
            let mut bytes = vec![0u8; n * shape.size];
            for (i, item) in bytes.chunks_exact_mut(shape.size).enumerate() {
                ItemShape::read(&shape.ty, b, item).map_err(|e| format!("[{i}]: {e}"))?;
            }
            return Ok(HeapValue::List(List::Plain(PlainList { shape, bytes })));
        }
        slot_type!(kind, T => T::read_codec(b).map(Slot::into_value))
    }
}

fn take<'a>(b: &mut &'a [u8], n: usize) -> Result<&'a [u8], String> {
    if b.len() < n {
        return Err("a heap value is cut short".into());
    }
    let (head, rest) = b.split_at(n);
    *b = rest;
    Ok(head)
}

fn put_str(s: &str, out: &mut Vec<u8>) {
    out.extend_from_slice(&(s.len() as u32).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

/// Where a component's heap fields are and what they hold; what its header points at. One per
/// distinct component name and layout, leaked once and shared by every world.
#[derive(Debug)]
pub struct HeapShape {
    pub size: usize,
    pub align: usize,
    pub fields: Vec<(usize, HeapKind)>,
    /// Values built through this shape and not yet dropped: for tests, which check every path
    /// frees what it built.
    live: AtomicIsize,
}

type ShapeKey = (String, usize, Vec<(usize, HeapKind)>);

impl HeapShape {
    /// The shape of `name` with `layout`, or `None` when it has no heap field.
    pub fn of(name: &str, layout: &ComponentLayout) -> Option<&'static HeapShape> {
        if !layout.has_heap() {
            return None;
        }
        static SHAPES: LazyLock<Mutex<HashMap<ShapeKey, &'static HeapShape>>> = LazyLock::new(Default::default);
        let fields: Vec<_> = layout.fields.iter().filter_map(|f| HeapKind::of(&f.ty).map(|k| (f.offset, k))).collect();
        let key = (name.to_string(), layout.size, fields);
        let mut shapes = SHAPES.lock().unwrap_or_else(|e| e.into_inner());
        let shape = shapes.entry(key).or_insert_with_key(|(_, size, fields)| {
            Box::leak(Box::new(HeapShape { size: *size, align: layout.align, fields: fields.clone(), live: AtomicIsize::new(0) }))
        });
        Some(*shape)
    }

    /// Values of this shape alive now.
    pub fn live(&self) -> isize {
        self.live.load(Ordering::SeqCst)
    }
}

/// Drops every heap value of a value that is no column's (a `Local`'s store).
///
/// # Safety
/// `p` points at a live value of `shape` that owns its heap values; they are dropped once, here.
pub unsafe fn drop_value(p: *mut u8, shape: &HeapShape) {
    // SAFETY: the caller's contract.
    unsafe { drop_values(p, shape) }
}

/// The shape a value's header points at.
///
/// # Safety
/// `p` points at a value of a component with a heap field.
pub unsafe fn header(p: *const u8) -> &'static HeapShape {
    // SAFETY: the header was written by `HeapBytes::new` from a `&'static HeapShape`.
    unsafe { &*std::ptr::read_unaligned(p as *const *const HeapShape) }
}

/// Drops every heap value of the value at `p`.
///
/// # Safety
/// `p` points at a value of `shape` that owns its heap values; they are dropped once, here.
unsafe fn drop_values(p: *mut u8, shape: &HeapShape) {
    for &(off, kind) in &shape.fields {
        // SAFETY: the slot holds a value of `kind` (the caller's contract).
        unsafe { drop_slot(p.add(off), kind) }
    }
    shape.live.fetch_sub(1, Ordering::SeqCst);
}

/// Drops the value in the slot at `p`.
///
/// # Safety
/// The slot holds a live value of `kind`, dropped once, here.
unsafe fn drop_slot(p: *mut u8, kind: HeapKind) {
    if let HeapKind::List(Item::Plain(shape)) = kind {
        // SAFETY: the caller's contract.
        return unsafe { Raw::get(p).free(shape) };
    }
    // SAFETY: the caller's contract.
    slot_type!(kind, T => drop(unsafe { std::ptr::read_unaligned(p as *const T) }))
}

/// Writes `v` into the slot at `p`, which holds nothing live.
///
/// # Safety
/// `p` has room for a heap slot.
unsafe fn write_slot(p: *mut u8, v: HeapValue) {
    if let HeapValue::List(List::Plain(l)) = &v {
        // SAFETY: the caller's contract.
        return unsafe { Raw::of(l.shape, &l.bytes).put(p, l.shape) };
    }
    let kind = v.kind();
    // SAFETY: the caller's contract.
    slot_type!(kind, T => unsafe { std::ptr::write_unaligned(p as *mut T, T::from_value(v)) })
}

/// A heap value at `p + off`, cloned.
///
/// # Safety
/// The slot holds a live value of `kind`.
pub unsafe fn read(p: *const u8, off: usize, kind: HeapKind) -> HeapValue {
    if let HeapKind::List(Item::Plain(shape)) = kind {
        // SAFETY: the caller's contract.
        return HeapValue::List(List::Plain(unsafe { Raw::get(p.add(off)).to_list(shape) }));
    }
    // SAFETY: read as a copy that is never dropped: the slot keeps owning it.
    slot_type!(kind, T => {
        let v = std::mem::ManuallyDrop::new(unsafe { std::ptr::read_unaligned(p.add(off) as *const T) });
        T::clone(&v).into_value()
    })
}

/// Whether the heap value at `p + off` equals `other`'s at the same offset, with no copy.
///
/// # Safety
/// The slot holds a live value of `kind`, and so does `other`'s.
pub unsafe fn eq_at(p: *const u8, off: usize, kind: HeapKind, other: &HeapBytes) -> bool {
    assert_eq!(other.slot(off), Some(kind), "offset {off} is not a {kind:?} slot");
    if let HeapKind::List(Item::Plain(shape)) = kind {
        // SAFETY: both slots hold live lists of `shape`.
        let (a, b) = unsafe { (Raw::get(p.add(off)), Raw::get(other.bytes.as_ptr().add(off))) };
        // SAFETY: as above.
        let (a, b) = unsafe { (a.items(shape), b.items(shape)) };
        let mut x = vec![0u8; shape.size];
        let mut y = vec![0u8; shape.size];
        return a.len() == b.len()
            && a.chunks_exact(shape.size).zip(b.chunks_exact(shape.size)).all(|(i, j)| {
                ItemShape::copy_item(&shape.ty, i, &mut x);
                ItemShape::copy_item(&shape.ty, j, &mut y);
                x == y
            });
    }
    // SAFETY: both are read as copies that are never dropped: the slots keep owning them.
    slot_type!(kind, T => unsafe {
        let a = std::mem::ManuallyDrop::new(std::ptr::read_unaligned(p.add(off) as *const T));
        let b = std::mem::ManuallyDrop::new(std::ptr::read_unaligned(other.bytes.as_ptr().add(off) as *const T));
        *a == *b
    })
}

/// Writes the heap value at `p + off` in the codec's wire format.
///
/// # Safety
/// The slot holds a live value of `kind`.
pub unsafe fn put_codec_at(p: *const u8, off: usize, kind: HeapKind, out: &mut Vec<u8>) {
    if let HeapKind::List(Item::Plain(shape)) = kind {
        // SAFETY: the caller's contract.
        let raw = unsafe { Raw::get(p.add(off)) };
        out.extend_from_slice(&(raw.len as u32).to_le_bytes());
        // SAFETY: as above.
        for item in unsafe { raw.items(shape) }.chunks_exact(shape.size) {
            ItemShape::put(&shape.ty, item, out);
        }
        return;
    }
    // SAFETY: as in `read`.
    slot_type!(kind, T => std::mem::ManuallyDrop::new(unsafe { std::ptr::read_unaligned(p.add(off) as *const T) }).put_codec(out))
}

/// Puts `v` at `p + off`, dropping the value it replaces.
///
/// # Safety
/// The slot holds a live value of `v`'s kind.
pub unsafe fn replace(p: *mut u8, off: usize, v: HeapValue) {
    // SAFETY: the caller's contract: the old value is dropped once, then the slot holds nothing.
    unsafe {
        drop_slot(p.add(off), v.kind());
        write_slot(p.add(off), v);
    }
}

/// Takes the value at `p + off`, leaving an empty one that owns nothing.
///
/// # Safety
/// The slot holds a live value of `kind`.
pub unsafe fn take_value(p: *mut u8, off: usize, kind: HeapKind) -> HeapValue {
    if let HeapKind::List(Item::Plain(shape)) = kind {
        // SAFETY: the caller's contract; the list is copied out, freed, and an empty one put.
        unsafe {
            let raw = Raw::get(p.add(off));
            let v = raw.to_list(shape);
            raw.free(shape);
            Raw::empty(shape).put(p.add(off), shape);
            return HeapValue::List(List::Plain(v));
        }
    }
    // SAFETY: the caller's contract; the empty value allocates nothing.
    slot_type!(kind, T => unsafe { replace_unaligned(p.add(off) as *mut T, T::default()) }.into_value())
}

/// `ptr::replace` for an unaligned slot.
///
/// # Safety
/// `p` holds a `T`.
unsafe fn replace_unaligned<T>(p: *mut T, v: T) -> T {
    // SAFETY: the caller's contract.
    unsafe {
        let old = std::ptr::read_unaligned(p);
        std::ptr::write_unaligned(p, v);
        old
    }
}

/// The drop fn a component with a heap field registers with: frees each heap value once.
///
/// # Safety
/// `ptr` owns a value of such a component.
pub unsafe fn drop_component(ptr: OwningPtr<'_>) {
    let p = ptr.as_ptr();
    // SAFETY: Bevy hands the drop fn an owned value of the component.
    unsafe { drop_values(p, header(p)) };
}

/// The clone fn a component with a heap field registers with (`cloneAndSpawn`): the clone owns
/// its own copy of every heap value.
pub fn clone_component(source: &SourceComponent, ctx: &mut ComponentCloneCtx) {
    let src = source.ptr().as_ptr();
    // SAFETY: the source is a live value of a component registered with this clone fn.
    let shape = unsafe { header(src) };
    // SAFETY: as above.
    let copy = unsafe { HeapBytes::copy_of(src, shape) };
    let mut buf = Aligned::new(copy.bytes());
    // SAFETY: `buf` holds a value of the component; the target takes ownership of its heap values
    // by copy, so `copy`'s own are forgotten, never dropped.
    unsafe { ctx.write_target_component_ptr(Ptr::new(buf.ptr())) };
    std::mem::forget(copy);
}

/// A copy of bytes in 16-aligned storage, as a column wants them.
pub(crate) struct Aligned(Vec<u128>);

impl Aligned {
    pub(crate) fn new(bytes: &[u8]) -> Self {
        let mut buf = vec![0u128; bytes.len().div_ceil(16).max(1)];
        // SAFETY: `buf` holds at least `bytes.len()` bytes.
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf.as_mut_ptr() as *mut u8, bytes.len()) };
        Aligned(buf)
    }

    pub(crate) fn ptr(&mut self) -> std::ptr::NonNull<u8> {
        std::ptr::NonNull::new(self.0.as_mut_ptr() as *mut u8).expect("a Vec's pointer is never null")
    }
}

/// A component value's bytes that own their heap values. Not `Clone`: a copy of its bytes would
/// own every value twice. Dropping it frees them; `commands::insert_owned` moves them into a
/// column instead.
#[derive(Debug)]
pub struct HeapBytes {
    bytes: Vec<u8>,
    shape: Option<&'static HeapShape>,
}

impl HeapBytes {
    /// Bytes of a component with no heap field.
    pub fn plain(bytes: Vec<u8>) -> Self {
        HeapBytes { bytes, shape: None }
    }

    /// Zeroed bytes of `size`; with a shape, its header and an empty value in each heap slot.
    pub fn new(size: usize, shape: Option<&'static HeapShape>) -> Self {
        let mut bytes = vec![0u8; size];
        if let Some(s) = shape {
            assert_eq!(s.size, size, "a heap shape's size");
            let p = bytes.as_mut_ptr();
            // SAFETY: `bytes` is `size` long, the header and every slot lie inside it.
            unsafe {
                std::ptr::write_unaligned(p as *mut *const HeapShape, s as *const HeapShape);
                for &(off, kind) in &s.fields {
                    write_slot(p.add(off), HeapValue::empty(kind));
                }
            }
            s.live.fetch_add(1, Ordering::SeqCst);
        }
        HeapBytes { bytes, shape }
    }

    /// A deep copy of the value at `src`.
    ///
    /// # Safety
    /// `src` points at a live value of `shape`, `shape.size` bytes long.
    pub unsafe fn copy_of(src: *const u8, shape: &'static HeapShape) -> Self {
        let mut out = HeapBytes::new(shape.size, Some(shape));
        let mut prev = HEADER;
        // Plain bytes between heap slots, then each heap value cloned.
        for &(off, kind) in &shape.fields {
            // SAFETY: the caller's contract.
            unsafe {
                std::ptr::copy_nonoverlapping(src.add(prev), out.bytes.as_mut_ptr().add(prev), off - prev);
                out.set(off, read(src, off, kind));
            }
            prev = off + HEAP_SLOT;
        }
        // SAFETY: as above.
        unsafe { std::ptr::copy_nonoverlapping(src.add(prev), out.bytes.as_mut_ptr().add(prev), shape.size - prev) };
        out
    }

    /// A deep copy: its own heap values.
    pub fn duplicate(&self) -> HeapBytes {
        match self.shape {
            None => HeapBytes::plain(self.bytes.clone()),
            // SAFETY: these bytes are a live value of `s`.
            Some(s) => unsafe { HeapBytes::copy_of(self.bytes.as_ptr(), s) },
        }
    }

    pub fn shape(&self) -> Option<&'static HeapShape> {
        self.shape
    }

    /// The bytes. A heap slot's bytes are a pointer this value owns: copy them only into a
    /// column through `commands::insert_owned`, never with a plain byte insert.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// The bytes of a value with no heap field; `Err` names why when it has one.
    pub fn into_plain(self) -> Result<Vec<u8>, String> {
        if self.shape.is_some() {
            return Err("a value with heap fields is not plain bytes".into());
        }
        let mut me = std::mem::ManuallyDrop::new(self);
        Ok(std::mem::take(&mut me.bytes))
    }

    /// The value as JSON, read through `layout`, which must be the one it was built with: a
    /// different size or different heap slots panic.
    pub fn decode(&self, layout: &ComponentLayout) -> serde_json::Value {
        assert_eq!(layout.size, self.bytes.len(), "a layout of another size");
        let slots: Vec<_> = layout.fields.iter().filter_map(|f| HeapKind::of(&f.ty).map(|k| (f.offset, k))).collect();
        assert_eq!(slots, self.shape.map(|s| s.fields.clone()).unwrap_or_default(), "a layout with other heap fields");
        // SAFETY: these bytes are a live value whose heap slots are `layout`'s.
        unsafe { crate::value::decode_component(layout, &self.bytes) }
    }

    /// A plain field's bytes, to write. Never a heap slot or the header: asks for one panic.
    pub fn field_mut(&mut self, offset: usize, len: usize) -> &mut [u8] {
        assert!(self.shape.is_none_or(|s| offset >= HEADER && s.fields.iter().all(|&(o, _)| offset + len <= o || offset >= o + HEAP_SLOT)));
        &mut self.bytes[offset..offset + len]
    }

    /// Replaces the heap value at `offset`, dropping the one there.
    pub fn set(&mut self, offset: usize, v: HeapValue) {
        assert!(self.slot(offset).is_some(), "offset {offset} is not a heap slot");
        // SAFETY: the slot holds a live value of its kind (built by `new`).
        unsafe { replace(self.bytes.as_mut_ptr(), offset, v) };
    }

    /// The heap value at `offset`, cloned.
    pub fn get(&self, offset: usize) -> Option<HeapValue> {
        let kind = self.slot(offset)?;
        // SAFETY: as in `set`.
        Some(unsafe { read(self.bytes.as_ptr(), offset, kind) })
    }

    fn slot(&self, offset: usize) -> Option<HeapKind> {
        self.shape?.fields.iter().find(|(o, _)| *o == offset).map(|(_, k)| *k)
    }

    /// Gives up the bytes without dropping their heap values: the caller has moved them (into a
    /// column).
    pub(crate) fn into_moved(self) -> Vec<u8> {
        let mut me = std::mem::ManuallyDrop::new(self);
        std::mem::take(&mut me.bytes)
    }
}

/// Equal shape, equal plain bytes and equal heap values.
impl PartialEq for HeapBytes {
    fn eq(&self, other: &Self) -> bool {
        match (self.shape, other.shape) {
            (None, None) => self.bytes == other.bytes,
            (Some(a), Some(b)) if std::ptr::eq(a, b) => a.fields.iter().all(|(o, _)| self.get(*o) == other.get(*o)) && {
                let plain = |x: &HeapBytes| {
                    let mut v = x.bytes.clone();
                    for (o, _) in &a.fields {
                        v[*o..*o + HEAP_SLOT].fill(0);
                    }
                    v
                };
                plain(self) == plain(other)
            },
            _ => false,
        }
    }
}

impl Drop for HeapBytes {
    fn drop(&mut self) {
        if let Some(s) = self.shape {
            // SAFETY: the bytes own their heap values (built by `new`), dropped once here.
            unsafe { drop_values(self.bytes.as_mut_ptr(), s) };
        }
    }
}

/// What a command needs to build a component with heap fields from JS's bytes: its layout, its
/// shape, and each heap field's default. JS never writes a heap slot; Rust fills each one from the
/// command's value or the default.
#[derive(Clone, Debug)]
pub struct HeapFill {
    pub name: String,
    pub layout: ComponentLayout,
    pub shape: &'static HeapShape,
    /// Per heap field, by index in the layout: its kind and default.
    pub heap: Vec<(usize, HeapKind, HeapValue)>,
}

impl HeapFill {
    /// The fill for a declaration, or `None` when it has no heap field. Defaults were validated.
    pub fn of(decl: &ComponentDecl, layout: &ComponentLayout) -> Option<HeapFill> {
        let shape = HeapShape::of(&decl.name, layout)?;
        let heap = layout
            .fields
            .iter()
            .enumerate()
            .filter_map(|(i, f)| {
                let kind = HeapKind::of(&f.ty)?;
                let default = decl.field(&f.name).and_then(|d| d.default.as_ref());
                let v = default.and_then(|d| crate::value::heap_value("", &f.ty, d).ok()).unwrap_or(HeapValue::empty(kind));
                Some((i, kind, v))
            })
            .collect();
        Some(HeapFill { name: decl.name.clone(), layout: layout.clone(), shape, heap })
    }

    /// A value from JS's bytes (`layout.size` of them: plain fields read, heap slots ignored)
    /// and `patch`: a `u16` count, then per heap field its `u16` index in the layout and its value
    /// in the codec's wire format. A heap field the patch leaves out takes its default.
    pub fn build(&self, js: &[u8], mut patch: &[u8]) -> Result<HeapBytes, String> {
        let mut out = HeapBytes::new(self.layout.size, Some(self.shape));
        for f in self.layout.fields.iter().filter(|f| !f.ty.is_heap()) {
            let w = f.ty.size();
            out.field_mut(f.offset, w).copy_from_slice(&js[f.offset..f.offset + w]);
        }
        let mut given = vec![false; self.layout.fields.len()];
        if !patch.is_empty() {
            let n = u16::from_le_bytes(take(&mut patch, 2)?.try_into().unwrap());
            for _ in 0..n {
                let i = u16::from_le_bytes(take(&mut patch, 2)?.try_into().unwrap()) as usize;
                let f = self.layout.fields.get(i).ok_or_else(|| format!("{} has no field {i}", self.name))?;
                let kind = HeapKind::of(&f.ty).ok_or_else(|| format!("{}.{} is not a heap field", self.name, f.name))?;
                let v = HeapValue::read_codec(kind, &mut patch).map_err(|e| format!("{}.{}: {e}", self.name, f.name))?;
                out.set(f.offset, v);
                given[i] = true;
            }
            if !patch.is_empty() {
                return Err(format!("{}: bytes left after its heap values", self.name));
            }
        }
        for (i, _, v) in &self.heap {
            if !given[*i] {
                out.set(self.layout.fields[*i].offset, v.clone());
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::StructField;
    use bevy::math::Vec3;

    #[test]
    fn the_vec_field_order_probe_reads_a_real_vec() {
        let mut v: Vec<u32> = Vec::with_capacity(9);
        v.extend_from_slice(&[4, 5, 6]);
        // SAFETY: a `Vec` is three words, read as a copy.
        let words: [usize; 3] = unsafe { std::mem::transmute_copy(&v) };
        let o = vec_order();
        assert_eq!([words[o[0]], words[o[1]], words[o[2]]], [v.as_ptr() as usize, v.capacity(), 3]);
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Hit {
        at: Vec3,
        by: Entity,
        kind: u8,
        hp: f32,
    }

    #[test]
    fn a_raw_list_is_a_rust_vec_a_system_can_push_onto() {
        let kind = FieldType::Enum { variants: vec!["hit".into(), "miss".into()], repr: None };
        let field = |name: &str, ty: FieldType| StructField { name: name.into(), ty };
        let ty = FieldType::Struct { fields: vec![field("at", FieldType::Vec3), field("by", FieldType::Entity), field("kind", kind), field("hp", FieldType::F32)] };
        let shape = ItemShape::of(&ty);
        assert_eq!((shape.size, shape.align), (size_of::<Hit>(), align_of::<Hit>()));
        let mut slot = [0u8; HEAP_SLOT];
        let p = slot.as_mut_ptr();
        let v = crate::value::heap_value("", &FieldType::List { of: Box::new(ty.clone()) }, &serde_json::json!([{ "at": [1, 2, 3], "by": 7, "kind": "miss", "hp": 0.5 }])).unwrap();
        let kind = v.kind();
        // SAFETY: `slot` is a heap slot holding nothing live; then it holds a list of `shape`.
        unsafe { write_slot(p, v) };
        assert_eq!(shape.live(), 1);
        // A Rust system takes it as a `Vec<Hit>`, pushes past its capacity and puts it back.
        // SAFETY: the slot holds a `Vec<Hit>`'s words.
        let mut hits: Vec<Hit> = unsafe { std::ptr::read_unaligned(p as *const Vec<Hit>) };
        assert_eq!(hits, vec![Hit { at: Vec3::new(1.0, 2.0, 3.0), by: Entity::from_bits(7), kind: 1, hp: 0.5 }]);
        hits.push(Hit { at: Vec3::ZERO, by: Entity::from_bits(9), kind: 0, hp: 2.0 });
        // SAFETY: back into the slot it came from.
        unsafe { std::ptr::write_unaligned(p as *mut Vec<Hit>, hits) };
        // SAFETY: the slot holds a live list of `kind`.
        let back = crate::value::heap_json(&unsafe { read(p, 0, kind) });
        assert_eq!(back[1], serde_json::json!({ "at": [0.0, 0.0, 0.0], "by": 9, "kind": "hit", "hp": 2.0 }));
        let mut wire = Vec::new();
        // SAFETY: as above.
        unsafe { put_codec_at(p, 0, kind, &mut wire) };
        assert_eq!(HeapValue::read_codec(kind, &mut wire.as_slice()).unwrap(), unsafe { read(p, 0, kind) }, "the codec's bytes read back");
        // SAFETY: as above; freed once, with the layout Rust grew it with.
        unsafe { drop_slot(p, kind) };
        assert_eq!(shape.live(), 0);
    }
}
