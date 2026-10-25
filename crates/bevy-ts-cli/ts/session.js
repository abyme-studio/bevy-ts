// `bevy-ts/session`: the store of session bytes. The param (SessionBytes) is exported from Rust;
// this module adds `get`, a copy of one value as a new array.

/** The value under `key` as a new `Uint8Array`, or `undefined`. */
export function get(session, key) {
  const n = session.size(key);
  if (n === undefined) return undefined;
  const out = new Uint8Array(n);
  session.read(key, out);
  return out;
}
