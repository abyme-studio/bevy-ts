## 1. FromWorld defaults

- [x] 1.1 `default_of` caches each type's `FromWorld` value and clones it (`reflect_value.rs`)
- [x] 1.2 Unit test: made once, cloned after
- [x] 1.3 `reflect_bench` before and after, 3 alternated rounds (`from-world-bench`); the numbers into `docs/reflection.md`

## 2. Landing

- [x] 2.1 `cargo test` for bevy-ts while working; the workspace build, tests and clippy once before landing
- [x] 2.2 `openspec validate from-world-default --strict`
