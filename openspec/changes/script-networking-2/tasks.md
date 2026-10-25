## 1. Dependencies and the size baseline

- [ ] 1.1 Before any dependency change: build the release prebuilt engine (`cargo static build --release -p bevy-ts-cli`) and record its stripped size and a clean-build time of `bevy-ts` in `bench/networking/results/size.md`.
- [ ] 1.2 `crates/bevy-ts/Cargo.toml`: `rustls` 0.23 (`default-features = false`, `ring`, `std`, `tls12`, `logging`), `rustls-platform-verifier` 0.7, `futures-rustls` 0.26 (`default-features = false`, `ring`, `tls12`). After the operator's go on design §4: `hyper` 1.12 with `client` and `http1`. Confirm `Cargo.lock` gains no `aws-lc-rs`, `aws-lc-sys` or `openssl`, keeps one `hyper` and one `tungstenite`, and list every crate it gains in the commit message.
- [ ] 1.3 Measure again as in 1.1 after sections 2-4 land: stripped size delta of the release engine and the clean-build time delta, split by TLS and by the HTTP client where the build allows. Record both in `bench/networking/results/size.md` and in `docs/networking.md`.

## 2. TLS

- [ ] 2.1 `crates/bevy-ts/src/net/tls.rs` (new): `NetTls` holding `Arc<rustls::ClientConfig>` with the `ring` provider set on the config (no process default), ALPN `http/1.1`; `Default` from the platform verifier; `NetTls::with_roots(pem)` for tests. A refusal reason in plain words from rustls's certificate error (not trusted, wrong name, expired).
- [ ] 2.2 `crates/bevy-ts/src/net/policy.rs`: `Target` takes `ws | wss | http | https`; drop `Refusal::Wss`; `HeadersNotLoopback` only for `ws:`, with the new text; add `Refusal::Downgrade`. Update the unit tests: `wss:` allowed, `headers` over `wss:` to a non-loopback host allowed, over `ws:` refused, `https:` default port 443, an entry's scheme not limiting schemes.
- [ ] 2.3 `crates/bevy-ts/src/net/socket.rs`: a `Plain | Tls` stream for `wss:` through `futures_rustls::TlsConnector` with the `NetTls` config and the URL host as server name (an IP host as an IP name); `SocketTransport::new(NetTls)`; certificate failures log the reason through a `NetEvent::Close` reason core logs.
- [ ] 2.4 `crates/bevy-ts/tests/it/fixtures/tls/`: a test CA and two leaf certificates (`127.0.0.1` with IP SAN, `localhost` only), valid until 2126, with the command that made them in `README.md`. Committed; no `rcgen`.
- [ ] 2.5 `crates/bevy-ts/tests/it/socket.rs`: a `wss:` echo server on `127.0.0.1:0` (sync `tungstenite` over `rustls::StreamOwned`); scenarios "A trusted local server", "An untrusted certificate", "TLS" (the handshake carries `authorization`).

## 3. Blob and process.argv

- [ ] 3.1 `crates/bevy-ts-v8/js/web.js`: `Blob` (parts, `type` normalisation, `slice` with negative indexes, `text`/`arrayBuffer`/`bytes`, copy on construction); `WebSocket.send(blob)`; `binaryType` `"blob"` wraps binary messages, other values ignored, the stage 1 warning removed.
- [ ] 3.2 `crates/bevy-ts/src/backend.rs`: `Generation.argv`; a `ScriptArgs` resource in core; the V8 backend fills `process.argv` once, like `env`.
- [ ] 3.3 `crates/bevy-ts-cli/src/cli.rs`: `parse_dev` keeps arguments after `--`; `play` takes `-- <args>`; `run_engine` plays the game beside the executable with every argument before matching a command. `play.rs`: `argv[1]` is the entry module's absolute path.
- [ ] 3.4 Tests: `crates/bevy-ts-v8/tests/it/web.rs` for "Building and slicing", "Binary as a Blob" (fake transport) and "Fetch is there"; `cli.rs` unit tests for `parse_dev` with `--` and for `play -- args`; a `play::tests` case for a shipped game with arguments; a dev headless test for "Arguments under dev" and "No arguments", including through the supervisor's project-engine path.

## 4. fetch (after the go on design §4)

- [ ] 4.1 `crates/bevy-ts/src/net/mod.rs`: `FetchId`, `HttpRequest`, `HttpResponse`, `NetRequest::Fetch`/`Abort`, `NetEvent::Response`/`FetchFailed`, `Transport::fetch`/`cancel`; the registry's fetch records (generation, hop count, original method); redirects followed in `take_batch` per the spec (allowlist and downgrade checks, 303/301/302 to `GET`, 307/308 keep, `authorization`/`cookie` dropped across hosts, 20 hops, `"error"`, `"manual"`); generations, `close_all`, `close_for_edit` and edit-mode refusals cover fetches; refusal and failure log lines prefixed `fetch <url>:`.
- [ ] 4.2 `crates/bevy-ts/src/net/fake.rs`: record each fetch (method, URL, headers, body); `respond(id, status, headers, body)`, `fail(id, reason)`; `cancelled(id)`.
- [ ] 4.3 `crates/bevy-ts/src/net/http.rs` (new): one exchange per call on `IoTaskPool`: DNS via `blocking::unblock`, TCP, TLS for `https:` with `NetTls`, hyper `client::conn::http1::handshake` over a `futures-io` adapter, the engine's own headers (`host`, `content-length`, `connection: close`, `accept-encoding: identity`, defaults for `accept` and `user-agent`), body read whole up to 16 MiB then `FetchFailed`; `cancel` drops the task.
- [ ] 4.4 `crates/bevy-ts-v8/src/lib.rs`: ops `op_bevyts_fetch(id, gen, method, url, headers_json, bytes)` and `op_bevyts_fetch_abort(id, gen)` to the outbox; `deliver` passes `Response` and `FetchFailed` records with the body as an `ArrayBuffer`.
- [ ] 4.5 `crates/bevy-ts-v8/js/web.js`: `Headers`, `Request`, `Response` (with `json`/`error`/`redirect` statics, `clone`, `bodyUsed`, no `body`), `fetch` (scheme, signal, body rules, default `content-type`s), abort rejecting at once, delivery settling by id, generation drops.
- [ ] 4.6 Tests through the fake transport, in `crates/bevy-ts/tests/it/net.rs` (core rules) and `crates/bevy-ts-v8/tests/it/net.rs` (end to end in V8): "A fetch not allowed", "A headless test of a fetch", every redirect scenario, "A huge download", "Saving during a fetch", "A fetch across edit", "A timeout" (`ManualDuration` at 16 ms), "Aborted before it starts", "The body is read once", `Headers` order and `getSetCookie`.
- [ ] 4.7 Real transport tests in `crates/bevy-ts/tests/it/http.rs` (new module in `tests/it/main.rs`) against a local HTTP/1.1 server on `127.0.0.1:0` (std `TcpListener`, hand-written responses) and the TLS fixtures: "The abyme client reads what is held" (headers seen on the wire), "A POST with a JSON body", chunked and close-delimited bodies, "Nothing listening", "The wrong name", an abort closing the server's connection.

## 5. Types and docs

- [ ] 5.1 `packages/bevy-ts/globals.d.ts`: `Headers`, `Request`, `Response` (no `body`), `fetch`, `Blob`, the init types, `process.argv`. `test/types-node` checks the scenario "A project that also has @types/node" with `pnpm check`.
- [ ] 5.2 `docs/networking.md`: the globals list, `wss:` and the verifier, `headers` over `wss:`, `fetch` (shape, what is missing, resolving at the end of the body, no reuse), redirects and `net.allow`, the 16 MiB body bound, `Blob`, `process.argv`, the refusal table, the fake transport's fetch handle, and the binary-size delta from 1.3. `docs/build.md`: a shipped game passes its arguments. README feature list.

## 6. Coverage

- [ ] 6.1 Every scenario in `specs/script-networking/spec.md` and `specs/shipped-game/spec.md` has a test, or is named here as covered by one above with the reason.
- [ ] 6.2 With a test abymed on its own port and state folder (never the live one on 4477), headless: `@abyme/client`'s `held` call runs through `fetch`. If no test abymed can run, say what is unverified.
