## 1. Examples

- [ ] 1.1 Add `example.json` to `examples/cube` (entry `src/game.ts`) and `examples/arena` (entry `src/game.tsx`)
- [ ] 1.2 Add `packages/bevy-ts/scripts/sync-examples.mjs`: copy each declaring example's shipped files into `packages/bevy-ts/examples/`, fail on one with anything under `native`; wire it as `prepack`; gitignore `packages/bevy-ts/examples/`; add `examples` to `files`

## 2. Launcher

- [ ] 2.1 Add `packages/bevy-ts/bin/examples.js`: read the shipped examples, `examples [--json]`
- [ ] 2.2 `new <dir> [--example <id>] [--no-install] [--json]`: validate, copy, write `package.json`, `tsconfig.json`, `.gitignore`; install with the runner from `npm_config_user_agent`, output on stderr
- [ ] 2.3 Dispatch `new` and `examples` in `bin/bevy-ts.js` before resolving the binary
- [ ] 2.4 The engine binary refuses `new` and `examples` with exit 2 and a pointer to `npx bevy-ts`; usage lists them

## 3. Tests

- [ ] 3.1 vitest: `examples --json` shape and order; no platform package needed
- [ ] 3.2 vitest: `new` for each shipped example with `--no-install`; files, pinned version, no `paths`; `tsc` passes with the local package linked
- [ ] 3.3 vitest: refusals (non-empty dir, unknown id) write nothing and exit 1; `--json` stdout is one object
- [ ] 3.4 vitest: `npm pack --dry-run --json` holds `examples/cube` and `examples/arena` and no `rapier`, `thrusters` or walk files
- [ ] 3.5 Rust: the binary's refusal of `new` and `examples`

## 4. Docs

- [ ] 4.1 README: a "Start" section, `npx bevy-ts new my-game` and `npx bevy-ts examples`
- [ ] 4.2 `docs/release.md`: the packed examples and how an example ships
