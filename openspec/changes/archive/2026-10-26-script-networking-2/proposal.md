## Why

Stage 1 (`openspec/changes/archive/2026-10-23-script-networking`) gave scripts `WebSocket` over `ws:` and nothing else, so a game reaches only a loopback server in clear text. `@abyme/client` also needs `fetch` (its `held` call is unverified for that reason), and a remote abymed, or any real service, needs TLS. This change finishes the web surface a network client expects: `wss:`, `fetch`, `Blob` and `process.argv`.

## What Changes

- **`wss:` on the existing `WebSocket`.** TLS through rustls, with the `ring` provider and the platform's certificate verifier. The `wss:` refusal goes. The `headers` option is allowed over `wss:` to any allowed host; over `ws:` it stays loopback only.
- **`fetch`, `Request`, `Response`, `Headers`.** The web's shape for `http:` and `https:`, HTTP/1.1 only, through the same registry, policy, generations, edit-mode rule and `Transport` seam as `WebSocket`. `net.allow` applies to the first URL and to every redirect hop. `AbortSignal` aborts a request. Bodies are whole, not streamed: no `response.body` stream.
- **`Blob`.** In memory: `size`, `type`, `slice`, `text`, `arrayBuffer`, `bytes`. A body for `fetch` and `WebSocket.send`; `binaryType = "blob"` now works (the default stays `"arraybuffer"`).
- **`process.argv`.** Node's shape: the engine's path, the entry, then the game's arguments. `bevy-ts dev <entry> -- <args>` and `bevy-ts play <folder> -- <args>` pass them; a shipped game passes every argument it is started with.
- **A shipped game started with arguments plays its game** instead of printing usage. The built-in commands (`dev`, `build`, …) are no longer reachable from an executable with a `game/` folder beside it.
- `typeof fetch` is `"function"`; stage 1's "no fetch yet" scenario is replaced.

## Capabilities

### New Capabilities

None. Everything extends `script-networking`.

### Modified Capabilities

- `script-networking`: the globals list gains `fetch`, `Request`, `Response`, `Headers`, `Blob` and `process.argv`; `WebSocket` accepts `wss:`, `Blob` sends and `binaryType = "blob"`; the allowlist requirement drops the `wss:` refusal and limits `headers` only on `ws:`; new requirements for TLS, `fetch`, redirects, `Blob` and `process.argv`; the testable-without-a-network requirement covers fetches.
- `shipped-game`: an engine with a `game/` beside it passes all its arguments to the game; `bevy-ts play` takes game arguments after `--`.

## Impact

- **Ask-first, for the operator's go (through lead@engine-core):**
  1. HTTP client: `hyper` 1.12's `client` feature. `hyper` (with `http1` and `server`) and `http-body-util` are already in every engine through `bevy_remote`; no crate joins `Cargo.lock` for HTTP. The alternative is a hand-rolled HTTP/1.1 client over `httparse` (also already locked): smaller by the client module, but our own chunked decoding and framing to keep correct. Design §4.
  2. `ring` 0.17 as rustls's provider. It compiles C and assembly with `cc`, which the build already needs (`blake3`, `sha2-asm`): no new toolchain, a longer clean build (measured in tasks). `aws-lc-rs`, rustls's default, is a much larger C build and binary. No pure-Rust provider is production-ready. Design §2.
  3. `futures-rustls` 0.26: the thin async adapter for rustls over `futures-io`, since `async-tungstenite`'s rustls features are tokio-only. Part of the rustls set, named here because it is a crate of its own.
  4. Who can reach what: `net.allow` checks every redirect hop before its DNS lookup; a redirect from `https:` to `http:` is refused; `Authorization` and `Cookie` are dropped on a redirect to another host. DNS stays unchecked after the allowlist: an allowed name may resolve to any address, as in stage 1. `HTTP_PROXY` and friends are ignored. No cookie jar, no CORS (a script has no origin).
  5. Performance: no connection reuse, so each `fetch` pays a TCP and TLS handshake; the binary grows (rustls, ring, the verifier, hyper's client); a response body is buffered whole and capped at 16 MiB. Each is measured or bounded in tasks.
  6. A shipped game with arguments now plays instead of printing usage.
- Pre-approved: `rustls` 0.23 (default features off, `ring`, `std`, `tls12`, `logging`) and `rustls-platform-verifier` 0.7 as its root store.
- Code: `crates/bevy-ts/src/net/` (`tls.rs`, `http.rs` new; `socket.rs`, `policy.rs`, `fake.rs`, `mod.rs`); `crates/bevy-ts/src/backend.rs` (`Generation.argv`); `crates/bevy-ts-v8/src/lib.rs` (fetch ops), `crates/bevy-ts-v8/js/web.js` (`Headers`, `Request`, `Response`, `fetch`, `Blob`, `process.argv`); `crates/bevy-ts-cli/src/cli.rs`, `play.rs` (arguments); `packages/bevy-ts/globals.d.ts`.
- Docs: `docs/networking.md` (wss, fetch, Blob, argv, redirects, the size delta), `docs/build.md` (shipped game arguments), README feature list.
- Tests: the fake transport for fetch, local servers only: a TLS server with a committed test CA, an HTTP/1.1 server; no outside network.
