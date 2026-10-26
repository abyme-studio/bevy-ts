## MODIFIED Requirements

### Requirement: Connections only to allowed hosts
The engine SHALL check every `WebSocket` URL against the allowlist before any DNS lookup or socket. An allowlist entry SHALL be `host` (any port), `host:port`, or a URL whose host and port are taken; host names compare case-insensitively and literally (`localhost` does not allow `127.0.0.1`). A refused URL SHALL open no socket: the constructor SHALL succeed, then `error` and `close` (1006) SHALL fire on the next delivery, and the log SHALL name the origin and say to add it to `net.allow` in `bevy-ts.json`. A `wss:` URL to an allowed host SHALL open over TLS, verifying the server's certificate against the webpki root store and the URL's host name; a failed verification SHALL fail the open as a dropped connection does (`error`, then `close` with 1006), the log naming the TLS error. An `https:` URL SHALL be read as `wss:`, as `http:` is read as `ws:`. A URL with a `headers` option SHALL be refused the same way as a host not allowed unless its host is loopback (`localhost`, `127.0.0.0/8`, `::1`), or its scheme is `wss:` and its host and port are the origin the embedding host named in `headersOrigin` (engine-embedding), the log saying headers are sent only to loopback or the host's named origin over `wss:`. An engine with no named origin (the CLI, an embedding without `headersOrigin`) SHALL send `WebSocket` headers to loopback, or over `wss:` to any allowed host. A `fetch`'s own headers SHALL follow fetch's rules, unchanged by `headersOrigin`: the device key never reaches a script's headers, as the host passes it as `originHeaders`, which the engine alone adds (engine-embedding). An empty allowlist SHALL refuse everything.

#### Scenario: Not allowed
- **WHEN** `net.allow` is `["127.0.0.1:4477"]` and a script opens `ws://example.com/`
- **THEN** no connection is attempted, `close` fires with 1006, and the log says `example.com` is not in `net.allow`

#### Scenario: A key to another machine
- **WHEN** `net.allow` has `10.0.0.5:4477` and a script opens `ws://10.0.0.5:4477/` with `{ headers: { authorization: "Bearer k" } }`
- **THEN** no connection is attempted, `close` fires with 1006, and the log says headers are sent only to loopback or the host's named origin over `wss:`

#### Scenario: TLS
- **WHEN** `net.allow` has `mac.tail.ts.net:8443`, the embedding host named `headersOrigin` `https://mac.tail.ts.net:8443`, and a script opens `wss://mac.tail.ts.net:8443/api/stream` with `{ headers: { authorization: "Bearer k" } }`
- **THEN** the connection opens over TLS and the handshake request carries the `authorization` header

#### Scenario: A key to another allowed TLS host
- **WHEN** `net.allow` has `mac.tail.ts.net:8443` and `other.example.com`, `headersOrigin` is `https://mac.tail.ts.net:8443`, and a script opens `wss://other.example.com/` with headers
- **THEN** no connection is attempted, `close` fires with 1006, and the log says headers are sent only to loopback or the host's named origin over `wss:`

#### Scenario: A certificate that does not verify
- **WHEN** a script opens an allowed `wss:` URL whose server presents a self-signed certificate
- **THEN** `error` fires, then `close` with 1006, and the log names the TLS error

### Requirement: Network and environment settings in the project file
`bevy-ts.json` SHALL accept `net.allow`, an array of allowlist entries, and `env`, an array of environment variable names. An entry of the form `${NAME}` SHALL be read from the environment when the engine starts; an unset or empty variable SHALL add nothing. `process.env` SHALL show exactly the names in `env` that are set, read when the engine starts, and nothing else; it SHALL be the same frozen-in-content object for the whole run. Both default to empty. `bevy-ts dev` SHALL read them when it starts the engine; a change to them SHALL apply when the engine next starts. `bevy-ts build` SHALL write both, unexpanded, into `game/game.json` as `net` and `env`, and a shipped game SHALL read them from there, expanding `${NAME}` from its own environment at start. An engine started by an embedding host SHALL take the environment from the host's `env` map instead of the process environment, for both `${NAME}` and `process.env`. A `game.json` without them SHALL mean both empty.

#### Scenario: The abyme client finds abymed
- **WHEN** `bevy-ts.json` has `"net": { "allow": ["127.0.0.1:4477", "${ABYME_URL}"] }` and `"env": ["ABYME_URL", "ABYME_KEY"]`, and the engine starts with `ABYME_URL=http://127.0.0.1:5000` and `HOME` set
- **THEN** a script may connect to `127.0.0.1:4477` and `127.0.0.1:5000`, `process.env.ABYME_URL` is `"http://127.0.0.1:5000"`, `process.env.ABYME_KEY` is `undefined`, and `process.env.HOME` is `undefined`

#### Scenario: A built game
- **WHEN** that project is built and the built game starts with `ABYME_URL` unset
- **THEN** `game/game.json` holds `"${ABYME_URL}"` as written, and the game may connect only to `127.0.0.1:4477`

#### Scenario: An embedded game
- **WHEN** that built game is created by an embedding host with `env: { "ABYME_URL": "https://mac.tail.ts.net:8443" }` while the process environment has `ABYME_URL=http://127.0.0.1:5000`
- **THEN** a script may connect to `mac.tail.ts.net:8443` and not to `127.0.0.1:5000`, and `process.env.ABYME_URL` is the host's value
