## 1. Setup holders

- [x] 1.1 A failing test: a setup system's `ResMut` and `Local` with a vector field, read and written
- [x] 1.2 `core.js`: a holder's nested views point at the holder; other copy-then-bind paths checked

## 2. Reserved event field names

- [x] 2.1 `reader.rs`: refuse `entity` (Event), `propagate` and `toJSON` (both kinds) at the field
- [x] 2.2 A test per name; `index.d.ts` and the ts-systems spec name the three
