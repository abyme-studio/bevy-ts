## Context

Stage 1 (`openspec/changes/archive/2026-10-23-script-networking`, `docs/networking.md`) built the path this change extends; there is no second one:

- JS (`crates/bevy-ts-v8/js/web.js`) queues requests through ops into an `OpState` outbox (`crates/bevy-ts-v8/src/lib.rs:139`).
- Core's `NonSend` `NetRegistry` (`crates/bevy-ts/src/net/mod.rs`) applies them through `NetPolicy` (`net/policy.rs`) to a `Transport`, polls it, bounds the queues, and hands one batch a frame to `ScriptBackend::deliver`.
- `SocketTransport` (`net/socket.rs`) runs one `IoTaskPool` task per connection: DNS through `blocking::unblock`, `async_io::Async<TcpStream>`, `async-tungstenite` 0.33 with only `handshake`. `FakeTransport` (`net/fake.rs`) serves tests.
- `wss:` is refused in `NetPolicy::check`; `Refusal::Wss` and `Refusal::HeadersNotLoopback` carry the log text.

What the lock already holds, checked with `cargo tree`: `hyper` 1.12 (`http1`, `server`), `http-body-util`, `httparse` and `smol-hyper`, all through `bevy_remote`'s `http` feature, which every engine builds. `cc` is already a build dependency (`blake3`, `sha2-asm`). No `rustls`, `ring` or `aws-lc` is in the lock.

## Goals / Non-Goals

**Goals:**
- `wss:` and `https:` verified as a browser would, with no way to turn verification off.
- `fetch` on the stage 1 registry, policy, generation and edit-mode rules, tested through the fake transport.
- `net.allow` holds for every hop of a redirect.
- `Blob` and `process.argv` as the web and Node shape them.
- The fewest new crates; the binary-size delta measured and written in the docs.

**Non-Goals:** HTTP/2, connection reuse, streamed bodies (`ReadableStream`, `Response.body`, upload progress), `FormData`, `File`, `TextEncoder`/`TextDecoder`, cookies, a proxy, CORS, a user-set CA or `--insecure` option, a server socket, compression, a limit on fetches in flight.

## Decisions

### 1. Root store: the platform verifier

`rustls-platform-verifier` 0.7 verifies through Security.framework on macOS, CryptoAPI on Windows, and the system's certificate files (`rustls-native-certs` plus `webpki`) on Linux.

Why: a shipped game lives for years. The platform's roots update with the OS; a baked `webpki-roots` snapshot goes stale in every game already shipped, and ignores a CA the player's machine trusts (a corporate proxy, a school network). On macOS and Windows it also checks revocation, as the browser does. It adds no C build: on Apple it links the system framework through `security-framework`.

Alternative: `webpki-roots`. Same answer on every machine and no OS dependency, but stale and blind to local trust. Rejected for a shipped game.

Tests cannot add a CA to the system store. So the TLS config is a value core owns: `NetTls` (`net/tls.rs`) holds one `Arc<rustls::ClientConfig>`. Its default uses the platform verifier; `NetTls::with_roots(pem)` builds one trusting only the given authorities, for tests. `install_socket(app, NetTls)` is the test hook; nothing in `bevy-ts.json` sets it.

### 2. Provider: `ring`

rustls 0.23 needs a crypto provider. Its default, `aws-lc-rs`, compiles AWS-LC: a large C build (and `prebuilt-nasm` on Windows), a larger binary. `ring` 0.17 compiles a small set of C and assembly with `cc`, which the build already requires for `blake3` and `sha2-asm`: no new toolchain, a slower clean build (task 1.3 measures it). Pure-Rust providers (`rustls-rustcrypto`, `graviola`) are experimental or limited to two architectures. So: `rustls` with `default-features = false, features = ["ring", "std", "tls12", "logging"]`, and `futures-rustls` with the same. Cost: no post-quantum key exchange (`X25519MLKEM768` needs `aws-lc-rs`). The provider is installed per config, never as a process default, so a native crate that brings its own is not disturbed.

### 3. TLS under the WebSocket: `futures-rustls`

`async-tungstenite`'s rustls features are tokio-only, so the socket transport wraps the TCP stream itself: `futures_rustls::TlsConnector::from(config).connect(server_name, Async<TcpStream>)`, then `client_async_with_config` on the TLS stream as on the plain one. `connect` in `socket.rs` gains a stream enum (`Plain | Tls`) implementing `AsyncRead + AsyncWrite`; the rest of the task is unchanged. `futures-rustls` is a thin adapter (no C, depends only on `rustls` and `futures-io`).

### 4. HTTP client: hyper's `client` feature (ask-first)

`fetch` needs an HTTP/1.1 client. Options, smallest first in what joins the lock:

| option | new crates | what it buys | cost |
| --- | --- | --- | --- |
| `hyper` 1.12, feature `client` | none (feature on a locked crate) | correct chunked, content-length and close-delimited framing; header parsing; 1xx handling | the client module, measured in task 1.3 |
| hand-rolled over `httparse` | none (`httparse` is locked) | the smallest binary | our own framing, chunked decoder and edge cases to test and keep |
| `ureq` / `reqwest` | many | redirects, pools | a sync client or tokio; ruled out |

Proposed: hyper's client. It is already in every engine for the server side; the delta is its `client::conn::http1` code. We drive it with `hyper::client::conn::http1::handshake` over our stream (a small `hyper::rt::Read`/`Write` adapter over `futures-io`, as `smol-hyper` does for the server) and spawn its connection future on `IoTaskPool`. No `hyper-util`, no pool. The alternative is the hand-rolled client; the operator's go decides (proposal, Impact 1).

### 5. fetch on the existing seam

`fetch` is a request kind next to `Open`, not a second path:

```rust
NetRequest::Fetch { id: FetchId, generation: u64, request: HttpRequest }   // method, url, headers, body
NetRequest::Abort { id: FetchId, generation: u64 }
NetEvent::Response { id: FetchId, response: HttpResponse }                 // status, reason, headers, body
NetEvent::FetchFailed { id: FetchId, reason: String }

trait Transport {
    // stage 1's four methods, then:
    fn fetch(&mut self, id: FetchId, request: &HttpRequest);   // one request, no redirect following
    fn cancel(&mut self, id: FetchId);
}
```

Fetch ids are a separate counter in JS, wire ids a separate counter in core. One transport call is one HTTP exchange on one connection: connect, TLS if `https:`, write, read the head and the whole body, close.

Redirects are followed in core, above the seam (`NetRegistry::take_batch`): a 3xx with `location` under `"follow"` becomes a new `transport.fetch` after `NetPolicy::check`, the downgrade check, the method rewrite and the header drop; no event reaches JS until the last hop. So the fake transport tests every redirect rule. The hop count rides on the registry's fetch record.

The body is read whole on the I/O side, up to 16 MiB (`MAX_MESSAGE_BYTES`, one owner for the bound); past it the task stops reading and reports `FetchFailed` with the bound, and core logs it. A response is one event, so the per-connection message bounds do not apply.

### 6. Policy

`NetPolicy::check` takes a target with a scheme in `ws | wss | http | https`. `Refusal::Wss` goes. `Refusal::HeadersNotLoopback` applies to `ws:` only; its text becomes "headers are sent in clear only over loopback; use wss:". A new `Refusal::Downgrade` ("a redirect may not leave TLS") and the `fetch` log prefix (`fetch https://…: …`). The allowlist stays host and port: an entry's scheme picks a default port and nothing else. The check stays before DNS; what a name resolves to is not checked, as in stage 1.

### 7. JS: `web.js`

- `Headers`: a sorted list of lower-cased pairs; `set-cookie` kept apart.
- `Request`, `Response`: the body held as a `Blob`-like byte store; `bodyUsed`; `clone()` copies before reading. `text()` and `json()` decode with `Deno.core.decode`.
- `fetch`: builds a `Request`, checks the scheme, the signal and a body with `GET`/`HEAD`, calls `op_bevyts_fetch(id, gen, method, url, headersJson, bytes)`, and keeps `{ resolve, reject, request }` by id. Delivery settles it. An abort rejects at once and calls `op_bevyts_fetch_abort`.
- `Blob`: one `Uint8Array` copy; strings through `Deno.core.encode`. `WebSocket.send(blob)` sends its bytes; `binaryType = "blob"` wraps a binary message.
- `process.argv`: filled once, like `env`, from `Generation.argv`.
- Generations: a fetch records the ambient generation; `commit`/`discard`/`restart` drop the records as they drop sockets, so their promises never settle.

### 8. Edit mode

`close_for_edit` also cancels every fetch and queues `FetchFailed { reason: "the game left play" }` for the first delivery in play, as it queues a socket's `close`. A fetch in edit mode is refused like an open.

### 9. `process.argv`

`Generation` gains `argv: Vec<String>` beside `env`, from a `ScriptArgs` resource set once when the engine starts. `parse_dev` stops at `--` and keeps the rest; `supervise::dev_in_project` already passes `args[1..]` through to the project engine. `play` takes `-- <args>`. In `run_engine`, an executable with `play::beside_exe()` plays at once with every argument, before the command `match`. `argv[1]` is the entry's absolute path: the entry file in dev, the game's entry module (`game/scripts/<entry>.js`) in play.

### 10. Types

`packages/bevy-ts/globals.d.ts` gains `Headers`, `Request`, `Response`, `fetch`, `Blob`, `BodyInit`, `HeadersInit`, `RequestInit`, `ResponseInit`, in the `interface` plus `declare var` shape `lib.dom.d.ts` and `@types/node` (through `undici-types`) both use, so they merge. `process` gains `argv: string[]` on `NodeJS.Process`. `Response.body` is left out. The `test/types-node` project checks the merge.

## Risks / Trade-offs

- [Binary size grows] → task 1.3 measures the release engine before and after and `docs/networking.md` records it; if the HTTP client dominates, the hand-rolled option is the fallback.
- [`ring`'s C build lengthens a clean build] → measured with the size; `cc` is already required.
- [No connection reuse: every fetch pays TCP and TLS handshakes] → fine for a game's occasional calls; a pool is a follow-up if a bench shows it matters.
- [Bodies are buffered, up to 16 MiB each, with no cap on fetches in flight] → bounded per fetch; a cap waits for a need.
- [An allowed name can resolve to a private address] → as stage 1; the allowlist is the operator's statement of trust.
- [The platform verifier differs per OS (revocation on macOS and Windows, not Linux)] → documented; the browser behaves the same way.
- [Resolving at the end of the body, not at headers] → only timing differs; documented.
- [`@types/node` declares `fetch`, `Blob` and friends too] → the merge test pins the current version.
- [A shipped game no longer reaches the CLI commands] → nothing ships needing them; `bevy-ts play` with the prebuilt engine still does.

## Migration Plan

Additive for scripts: stage 1 code runs unchanged (`binaryType` still defaults to `"arraybuffer"`); a script that relied on `typeof fetch === "undefined"` sees a function. A shipped game started with arguments now plays. No protocol change. Rollback is a revert.

## Open Questions

- The HTTP client: hyper's `client` feature or a hand-rolled client (proposal, Impact 1). Build waits on that answer for tasks 4.x only; the TLS, `Blob` and `argv` tasks do not depend on it.
