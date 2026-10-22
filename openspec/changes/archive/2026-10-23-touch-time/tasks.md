## 1. Protocol

- [x] 1.1 `protocol/bevy-ts.openrpc.json`: optional `t` on `TouchEvent`; bump to 1.20.0
- [x] 1.2 Regenerate Rust and TS types (`scripts/gen-protocol.sh`)

## 2. Tests and docs

- [x] 2.1 `crates/bevy-ts-viewport/src/input.rs`: a touch with `t` and one without both become `TouchInput`
- [x] 2.2 Delta spec for `viewport-input` names `t`
