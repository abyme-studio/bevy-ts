/**
 * The value under `key` as a new `Uint8Array` (a copy), or `undefined`. Two host calls and one
 * allocation: for many keys a frame, `session.read` into one buffer instead.
 *
 * The store survives every reload, failed ones too; a `restart: true` reload and a Stop empty it.
 * It is one store for each app, never written to disk. A key is 1 to 1,024 bytes of UTF-8; the
 * store holds at most 64 MiB, keys included.
 */
export declare function get(session: SessionBytes, key: string): Uint8Array | undefined;
