//! Session bytes: string keys to byte values, held in the `World` beside `SystemMemory`, never in
//! the JS heap. Every reload keeps them; a restart and a Stop clear them, as they clear `Local`s.

use bevy::prelude::*;
use std::collections::HashMap;

/// The most bytes of UTF-8 a key takes.
pub const KEY_MAX_BYTES: usize = 1024;
/// The most bytes the store holds, keys and values counted.
pub const STORE_MAX_BYTES: usize = 64 << 20;

/// The store. TS reaches it as the `SessionBytes` param of `bevy-ts/session`; Rust as
/// `Res<SessionBytes>` / `ResMut<SessionBytes>`.
#[derive(Resource, Default, Debug)]
pub struct SessionBytes {
    values: HashMap<String, Box<[u8]>>,
    bytes: usize,
}

/// A key as an error names it: cut to 64 characters.
fn shown(key: &str) -> String {
    match key.char_indices().nth(64) {
        Some((i, _)) => format!("{:?}...", &key[..i]),
        None => format!("{key:?}"),
    }
}

impl SessionBytes {
    /// Stores a copy of `value` under `key`, replacing the old one. Past a limit it errs and
    /// changes nothing; a replaced value is not counted.
    pub fn put(&mut self, key: &str, value: &[u8]) -> Result<(), String> {
        if key.is_empty() || key.len() > KEY_MAX_BYTES {
            return Err(format!("key {} is {} bytes; a key is 1 to {KEY_MAX_BYTES} bytes of UTF-8", shown(key), key.len()));
        }
        let old = self.values.get(key).map_or(0, |v| key.len() + v.len());
        let next = self.bytes - old + key.len() + value.len();
        if next > STORE_MAX_BYTES {
            return Err(format!(
                "key {}: a {}-byte value would make the store {next} bytes, past its {STORE_MAX_BYTES}-byte (64 MiB) limit",
                shown(key),
                value.len()
            ));
        }
        self.values.insert(key.to_string(), value.into());
        self.bytes = next;
        Ok(())
    }

    /// The value under `key`.
    pub fn get(&self, key: &str) -> Option<&[u8]> {
        self.values.get(key).map(|v| &**v)
    }

    /// The length of the value under `key`.
    pub fn size(&self, key: &str) -> Option<usize> {
        self.values.get(key).map(|v| v.len())
    }

    /// Removes the value under `key`; whether one was there.
    pub fn delete(&mut self, key: &str) -> bool {
        match self.values.remove(key) {
            Some(v) => {
                self.bytes -= key.len() + v.len();
                true
            }
            None => false,
        }
    }

    pub fn clear(&mut self) {
        self.values.clear();
        self.bytes = 0;
    }

    /// How many keys.
    pub fn count(&self) -> usize {
        self.values.len()
    }

    /// The bytes held, keys and values.
    pub fn bytes(&self) -> usize {
        self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_replaces_counts_without_old() {
        let mut s = SessionBytes::default();
        s.put("a", &[1, 2, 3]).unwrap();
        s.put("bb", &[4]).unwrap();
        assert_eq!((s.count(), s.bytes()), (2, 1 + 3 + 2 + 1));
        s.put("a", &[9; 10]).unwrap();
        assert_eq!((s.count(), s.bytes(), s.get("a")), (2, 1 + 10 + 2 + 1, Some(&[9u8; 10][..])));
        assert!(s.delete("a"));
        assert!(!s.delete("a"));
        assert_eq!((s.count(), s.bytes(), s.size("bb")), (1, 3, Some(1)));
        s.clear();
        assert_eq!((s.count(), s.bytes()), (0, 0));
    }

    #[test]
    fn limits_leave_store_unchanged() {
        let mut s = SessionBytes::default();
        s.put("keep", &[1]).unwrap();
        let e = s.put("", &[1]).unwrap_err();
        assert!(e.contains("1 to 1024 bytes"), "{e}");
        let long = "k".repeat(KEY_MAX_BYTES + 1);
        let e = s.put(&long, &[1]).unwrap_err();
        assert!(e.contains("1025 bytes") && e.contains(&format!("{:?}...", "k".repeat(64))), "{e}");
        s.put(&"k".repeat(KEY_MAX_BYTES), &[]).unwrap();
        s.delete(&"k".repeat(KEY_MAX_BYTES));

        // A replacing put is counted without the value it replaces.
        let big = vec![0u8; STORE_MAX_BYTES - 4 - 1 - 3];
        s.put("big", &big).unwrap();
        assert_eq!(s.bytes(), STORE_MAX_BYTES);
        s.put("big", &big).unwrap();
        let e = s.put("keep", &[1, 2]).unwrap_err();
        assert!(e.contains("\"keep\"") && e.contains("2-byte value") && e.contains("67108864"), "{e}");
        assert_eq!((s.count(), s.bytes(), s.get("keep")), (2, STORE_MAX_BYTES, Some(&[1u8][..])));
    }
}
