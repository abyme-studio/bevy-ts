# TLS test fixtures

A test certificate authority and two server certificates it signed, for the real transport's
`wss:` and `https:` tests (`socket.rs`, `http.rs`). Only those tests trust `ca.pem`, through
`NetTls::with_roots`; the platform's trust store never sees it.

| file | what |
| --- | --- |
| `ca.pem` | the authority, `CN=bevy-ts test CA` |
| `ip.pem`, `ip.key` | a leaf for `127.0.0.1` (an IP name only) |
| `localhost.pem`, `localhost.key` | a leaf for `localhost` (a DNS name only): "The wrong name" fetches it at `127.0.0.1` |

ECDSA P-256, valid until 2126. Made with `./make.sh` (OpenSSL); the authority's key is thrown
away, so remaking one file means remaking all of them. Test keys: never trust them anywhere else.
