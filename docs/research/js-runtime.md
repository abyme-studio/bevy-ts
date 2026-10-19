# JS runtime for bevy-ts

Recommendation: V8 through `deno_core` 0.412 (`v8` crate 152.2) for desktop, behind a bridge that only knows "call a system, hand it column views, take back a command buffer", so the web build can use the browser's engine and a QuickJS-ng backend can be added later for iOS and consoles.

Researched 2026-10-19. Versions are from crates.io and GitHub on that date.

| Engine (Rust crate, latest) | JIT | Native memory as ArrayBuffer, no copy | Debugger | Threads | Engine size | iOS / consoles / web | Maturity | Verdict |
|---|---|---|---|---|---|---|---|---|
| V8 via `deno_core` 0.412.0 (2026-09-16) | Yes (Sparkplug, Maglev, TurboFan) | Yes: `new_backing_store_from_ptr`, with the V8 sandbox off (the default) | Chrome DevTools / VS Code over CDP, built in | One isolate per thread; `JsRuntime` is `!Send` | about 38 MB static lib (gz) per desktop target | iOS: jitless only, prebuilt lib exists. Consoles: no official build. Web: no | Ships Deno; repo merged into `denoland/deno` 2026-02-27, crate still released every ~3 weeks | Pick |
| V8 via `v8` (rusty_v8) 152.2.0 (2026-08-20) | Yes | Same | CDP, but you build the server | Same | Same | Same | Stable since 129.0.0; new major every 4 weeks | Fallback if deno_core's tokio and op macros get in the way |
| QuickJS-ng via `rquickjs` 0.14.0 (2026-09-18); quickjs-ng 0.17.0 (2026-09-18) | No (bytecode interpreter) | Yes: `ArrayBuffer::from_source` / `from_source_shared` take ownership of a Rust buffer | None upstream | Runtime per thread; optional `parallel` feature (marked experimental) | about 1 MB | Runs everywhere C does, wasm included; shipped on every console by Minecraft Bedrock's script API | Active, stable API | Runner-up |
| Boa `boa_engine` 0.22.0 (2026-08-28) | No | Through its own `JsArrayBuffer`, not foreign memory | None | `!Send` | Pure Rust, a few MB | Anywhere Rust builds | ~94% Test262 at v0.21; GC is being rewritten in 2026 | Too slow: 228 against QuickJS's 1227 on Boa's own V8-suite run, 2026-10-19 |
| Nova `nova_vm` 1.0.0 (2026-03-15) | No | No stable API for it | None | n/a | Pure Rust | Anywhere Rust builds | README: performance "acceptable, but it is not fast by any means"; no Wasm, incomplete RegExp | Research only |
| JavaScriptCore (no cross-platform Rust crate; `javascriptcore-rs` 2.0 is WebKitGTK only) | Yes on desktop. No JIT through the JSC API on iOS, and none on macOS for hardened-runtime apps | Yes (`JSObjectMakeArrayBufferWithBytesNoCopy`) | Safari Web Inspector, not CDP | Context group per thread | System on Apple, about 48 MB static elsewhere | iOS system engine, interpreter only. Windows means building WebKit yourself | Mature engine, no maintained Rust binding | Only as an iOS backend |
| Hermes V1 (React Native default since 0.84, 2026-02-11) | No: V1 has no JIT and no typed AOT yet | Yes via JSI `MutableBuffer` | CDP (RN DevTools) | One runtime per thread | a few MB | Built for iOS/Android | Mature in React Native, but C++ JSI only; `rusty_hermes` is 0.0.0 | No: same speed class as QuickJS with a harder bridge |
| SpiderMonkey via `mozjs` (mozjs-sys 153.4, 2026-10-16) | Yes | Yes (`NewExternalArrayBuffer`) | Firefox remote protocol only, no turnkey server | Context per thread | Large, builds from source | Desktop; no iOS JIT; no consoles | Maintained for Servo, unsafe Servo-shaped API | No: V8's costs without its tooling |
| Porffor alpha-17 (2026-10-10) | AOT JS to Wasm/C | n/a | None | n/a | Small | Wasm/C output | "Research project, not yet intended for serious use"; no `eval`, so no hot reload | Watch only (2657 on the same benchmark, but it can't reload) |
| `v8x` 149.4.0-rc (2026-08) | Depends on engine | Claims the rusty_v8 API over V8, JSC or QuickJS-ng | Depends on engine | Same as rusty_v8 | V8 40 MB, JSC 48 MB, system JSC 0, QuickJS-ng 1 MB (its README) | Would give iOS/consoles without a second bridge | Release candidate, 44 stars, one author (a Deno core developer) | Track it; it could remove the trade-off |

## What decides it

- **The hot path is JS looping over typed arrays, not calls into Rust.** With columns exposed as `Float32Array` views and one call per system per frame, the crossing is cheap on every engine. What is left is the loop, and a JIT wins that. V8's own numbers put jitless 40% (Speedometer) to 80% (Web Tooling) slower than JIT, and tight numeric loops lose more. QuickJS is about half of jitless V8 (1227 against 2562 on Boa's run, 2026-10-19).
- **V8 fast API calls don't decide anything here.** They only run from optimized code, and typed arrays are still listed as "to be supported" argument types in `v8-fast-api-calls.h`. Design for few calls carrying buffers, not many small calls.
- **Zero-copy needs the V8 sandbox off.** `v8_enable_sandbox` is an opt-in feature of the crate. With it on, `new_backing_store_from_ptr` only accepts memory inside the sandbox, and Node's sandbox build has to copy for the same reason. Keep it off, which is the default. Never turn it on without redesigning the bridge.
- **Column views live for one system run.** Bevy tables reallocate and entities move between archetypes. Build the backing stores from column pointers right before a JS system runs, defer structural changes to the command buffer, and detach the buffers afterwards. This applies to every engine.
- **Hot reload is cheap because Rust owns the state.** Components live in the ECS, so a reload can throw the JS context away and evaluate the new modules. No engine-level module replacement is needed. Deno's `--watch-hmr` (`Debugger.setScriptSource`) is a bonus for keeping closures alive, not a requirement.
- **TypeScript is a host-side compile step on every engine.** No engine here strips types. Use `oxc_transformer` 0.154 (2026-10-13) or `deno_ast` 0.53.3 (swc, 2026-07-09) in the `bevy-ts dev` process, plus a custom JSX runtime. `deno_core` does not handle TS on its own.
- **Threading: one isolate, one batch stage.** Isolates and runtimes are `!Send` in every Rust binding (rquickjs's `parallel` feature aside), so JS systems run as a main-thread or exclusive stage while Rust systems run in parallel around it. Parallel JS means one isolate per worker with no shared JS heap. That is a later optimisation, not a reason to pick an engine. bevy-react runs V8 on its own thread for UI and measures about 1 ms (1k nodes) to 6 ms (10k nodes) per thread hop. That is fine for a reconciler and wrong for per-frame systems.
- **Debugger: only V8 and Hermes speak CDP out of the box.** On QuickJS-ng, TS authors get no breakpoints, which costs more day to day than the speed gap.
- **The platforms outside desktop shrink V8's lead.** iOS allows no JIT for third-party engines, or for JSC through its API. Consoles forbid writable executable memory. On the web, the engine is the browser's own. So the bridge must be engine-neutral at the "buffers in, command buffer out" level. Bevy's wasm memory is already an ArrayBuffer, so the web backend gets zero-copy for free. A notarized macOS build needs the `com.apple.security.cs.allow-jit` entitlement for V8.

## Prior art

- **bevy-react 0.8.1 (2026-10-14, Bevy 0.20):** React/TSX to `bevy_ui` over `deno_core` natively, and the browser's engine on wasm. One batch of ops per React commit; JSON flush is about 0.5 µs per op, and JS reconciliation dominates. This is close to bevy-ts's TSX half and confirms deno_core plus a web fallback.
- **bevy_superui / `superui_js` 0.4.0 (2026-10-10):** a "JS engine boundary (V8 native, browser web)" plus a Solid-style TSX transpiler. Same split as above, with fine-grained reactivity.
- **bevy_mod_scripting 0.23.0 (2026-10-14):** Lua, Luau and Rune through `bevy_reflect`, with no JS and no wasm. Its reflection-per-access model is what bevy-ts avoids. Its README still says "not yet optimized".
- **bevy_mod_js_scripting:** `deno_core` natively and the browser on the web, as early as 2022. Abandoned (last push 2023-01-01), but it was the same architecture.
- **Cocos Creator 3.x:** V8 natively. To avoid JSB calls, node transforms live in memory shared between C++ and JS (`NativeMemorySharedToScriptActor`), and the scene graph moved to C++. That is the same lesson as bevy-ts's batching rule.
- **GodotJS 1.0:** one binding over V8, QuickJS, QuickJS-ng, JSC and the browser. CDP debugging works only on V8, and only Safari's inspector on JSC. A multi-engine backend is practical, and the debugger is what each engine costs.
- **PuerTS (Unity_v3.0.3, 2026-09-05):** V8 by default "best overall performance", with QuickJS "for size-sensitive" builds. The same split this note recommends.
- **Minecraft Bedrock Script API:** QuickJS-based, ships on Xbox, PlayStation, Switch and mobile. It is the proof that QuickJS covers consoles.

## Sources

- crates.io API, 2026-10-19: deno_core, v8, rquickjs, boa_engine, nova_vm, mozjs, javascriptcore-rs, deno_ast, oxc_transformer, bevy_mod_scripting, bevy-react, superui_js, v8x
- rusty_v8 release v152.2.0 assets (2026-08-20): https://github.com/denoland/rusty_v8/releases/tag/v152.2.0
- rusty_v8 Cargo features (`v8_enable_sandbox` opt-in): https://github.com/denoland/rusty_v8/blob/main/Cargo.toml
- `ArrayBuffer` sandbox notes, v8 152.2.0: https://docs.rs/v8/latest/v8/struct.ArrayBuffer.html
- Node.js V8 sandbox build PR: https://github.com/nodejs/node/pull/62237
- rusty v8 stabilized: https://deno.com/blog/rusty-v8-stabilized
- deno_core merged into denoland/deno: https://github.com/denoland/deno_core
- V8 fast API header: https://chromium.googlesource.com/v8/v8/+/refs/heads/main/include/v8-fast-api-calls.h
- JIT-less V8: https://v8.dev/blog/jitless
- Deno HMR: https://docs.deno.com/runtime/run/watch_mode/ , https://docs.deno.com/runtime/desktop/hmr/
- Boa benchmark data, run 2026-10-19: https://github.com/boa-dev/data/blob/main/bench/results/score.json
- Boa v0.21 release: https://boajs.dev/blog/2025/10/22/boa-release-21 ; Boa 2026 roadmap: https://boajs.dev/roadmap
- Nova README: https://github.com/trynova/nova
- rquickjs ArrayBuffer 0.14.0: https://docs.rs/rquickjs/latest/rquickjs/struct.ArrayBuffer.html
- quickjs-ng releases: https://github.com/quickjs-ng/quickjs/releases
- React Native 0.84, Hermes V1 default (2026-02-11): https://reactnative.dev/blog/2026/02/11/react-native-0.84
- Software Mansion on Hermes V1: https://swmansion.com/blog/welcoming-the-next-generation-of-hermes-67ab5679e184
- JSC without JIT on iOS: https://lucid.co/techblog/2019/01/03/javascriptcore-10-months-later , https://dev.to/alastaircoote/to-jsc-or-not-to-jsc-running-javascript-on-ios-in-2020-44ba
- mozjs: https://github.com/servo/mozjs
- Porffor: https://github.com/CanadaHonk/porffor
- v8x: https://github.com/littledivy/v8x
- bevy-react: https://tulustul.github.io/bevy-react/how-it-works/ , https://tulustul.github.io/bevy-react/performance/
- bevy_mod_scripting: https://github.com/makspll/bevy_mod_scripting
- bevy_mod_js_scripting: https://github.com/jakobhellermann/bevy_mod_js_scripting
- Cocos JSB optimizations: https://docs.cocos.com/creator/manual/en/advanced-topics/jsb-optimizations.html
- GodotJS: https://github.com/godotjs/GodotJS
- PuerTS: https://www.nuget.org/packages/Puerts.Core.Complete , https://github.com/Tencent/puerts
- Minecraft Bedrock scripting engine: https://wiki.bedrock.dev/scripting/api-environment
