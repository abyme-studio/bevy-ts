## 1. Refuse a wrong element type

- [x] 1.1 `host.rs` glue: `H.bytes($arg, <view class>, NAME, method, arg)` for each slice argument
- [x] 1.2 `core.js` `HOST.bytes`: refuse anything not an instance of the class, naming param, method, argument, expected and given types; sync `packages/bevy-ts/runtime/core.js`
- [x] 1.3 Tests: `cameras.rs` worldsToViewport with a `Float64Array` as `points` and as `out`; `exported.rs` castRays with a `Float64Array` and an `ArrayBuffer`, neither crossing
- [x] 1.4 `docs/native.md`: what a slice argument refuses
