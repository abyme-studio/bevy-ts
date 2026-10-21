# Scene bindings: may `prop={() => expr}` sit on a scene element, or only in UI?

Recommendation: allow a binding on any element, scene or UI, but define it as a derived field with one owner, and run bindings on native-layout components as one generated system per binding site (Bevy's own change filter over the site's instances, one closure, flyweight rows, compare before write), never as one Solid effect per instance. "UI only" draws the line in the wrong place; "one owner, no per-instance cost" draws it where the problems are.

Researched and measured 2026-10-19 on this Mac (Apple M5 Max, macOS 25.4; deno 2.9.7 / V8 15.0.245, rustc 1.98, `bevy_ecs` 0.20). Benchmarks in `bench/bindings/` (`fire-paths.js`, `rust/`), raw output and tables in `bench/bindings/results/`. Pending the operator; see the last section for what to revert if the ruling goes the other way.

## The question from scratch

A light should glow when an alarm is on. Two ways to say it in bevy-ts:

```tsx
// A binding, inside the prefab. Four extra characters: `() =>`.
export const Siren = () => {
  const self = ref();
  const alarm = useComponent(self, Alarm);
  return (
    <entity ref={self}>
      <Alarm initialOn={false} />
      <PointLight intensity={() => (alarm().on ? 8 : 2)} />
    </entity>
  );
};
```

```ts
// A system, in a separate file, over every entity with both components.
app.system("siren", { read: [Alarm], write: [PointLight] }, (q) => {
  for (const [a, l] of q.changed(Alarm)) l.intensity = a.on ? 8 : 2;
});
```

Both are fine for one siren. The open question in `openspec/changes/archive/2026-10-21-tsx` is whether the first form should be allowed on scene elements at all (today: "allowed for now, with a dev warning"), because five things hang on it:

1. Cost when `Siren` is instanced 10,000 times and all of them change.
2. Who may write `intensity`: the binding, a system, the IDE, a `.bsn` level?
3. What the viewport shows for a bound prop, and whether a level may override it.
4. What a hot reload does to it.
5. When a light is at 8 and you did not expect it, how you find out who set it.

The three options:

**A. UI only.** `<text>{() => ...}</text>` binds; `<PointLight intensity={() => ...}>` is a type error and a load error. Behaviour on scene entities is systems, always.

```tsx
<text>{() => `HP ${health().hp}`}</text>          // ok
<PointLight intensity={() => (alarm().on ? 8 : 2)} />  // error: write the siren system
```

**B. Anywhere, as Solid effects (the current draft).** Each mounted instance gets a signal, an effect and a closure (tsx decisions 3 and 9). Rust checks each subscription's change tick per frame; when some moved, one host call delivers their values, effects re-run, each emits a patch op, Rust applies the ops.

**C. Anywhere, as derived fields run per site (recommended).** The binding means "this field is a function of what I read, and I am its only writer". A *site* is the element and prop in source, one per prefab element however many instances exist. Bindings on native-layout components run as a generated system per site:

- Rust finds the site's instances whose reads changed: the instance's own components by their change ticks in table order (what `Changed<T>` does), another entity's components through a per-pair check (decision 9's path), a signal through its version (a change re-runs every instance of the site). No JS call when nothing changed.
- One host call per frame for all sites with changes, carrying the changed rows. JS runs the site's one closure per row through the flyweight accessors a TS system uses (bridge decision 5), compares with the current value and writes through the column view with the run's tick (bridge decision 6). A derived value that did not change must not move `Changed<PointLight>`.
- A site's reads are fixed at its first evaluation, as a system's query is. A later run that reads something new is a dev error naming the site.
- The reconciler never writes a bound field from a literal. A TS system whose `write` set holds the component warns in dev naming the system and the site. The viewport shows a bound prop read-only with its expression and site location.
- Bindings on reflected components (`Text`, `Node`) keep decision 9's path: per-pair subscriptions, `patchReflect`. In practice that is UI, but the split is by target component kind, which the runtime can check, not by "is this UI", which it cannot.

```tsx
// C: the same TSX as the binding sample above. It compiles to, in effect:
//   per site "siren.tsx:8 PointLight.intensity": read Alarm (self), write PointLight.intensity;
//   each frame: for each instance whose Alarm tick moved, intensity = closure(row), if different.
```

## What the engines learned

- **react-three-fiber** (the closest cousin: JSX props on scene objects). Its pitfalls page is a list of what people do wrong with exactly this feature: "Avoid setState in loops", "Do not bind to fast state reactively", "Fast updates are carried out in `useFrame` by mutation". Its ownership rule is the one C adopts: "Consider mutating props safe as long as the component is the only entity that mutates." The lesson is not "no props on scene objects"; it is "a prop is a declaration, and anything per frame or with two writers goes elsewhere".
- **Solid.** "Components, much like other functions, will only run *once*"; only reads inside a tracking scope update. Fine-grained reactivity is cheap because a UI has hundreds of bindings, not hundreds of thousands. Nothing in Solid is instanced 100k times.
- **Godot.** No bindings in the engine. A `.tscn` is data; behaviour is `_process` in a script. An instance edit in the parent scene is an override: "Changing a property on an instance always overrides values from the corresponding packed scene", with a grey revert button beside it. Code that writes scene values while editing (`@tool`) is allowed and the docs warn: "Modifications in the editor are permanent, with no undo/redo possible." The community's data-binding addons (DataBind Controls, Bindora) target Control nodes, that is UI.
- **Unity.** UI Toolkit's runtime binding binds a C# object "to the properties of a UI control": UI only, and change-driven by versioning. Its docs are candid about the naive form: "By default, active binding objects are updated every frame, which can be a resource-intensive process", so "the binding system doesn't update a binding object if the version of its data source remains unchanged", and `INotifyBindablePropertyChanged` narrows the update to the changed path. Scene objects have no bindings: a `MonoBehaviour` writes `light.intensity` in `Update()`, and the Inspector field it fights snaps back, which every Unity user has hit once.
- **Unreal.** UMG property and function bindings were polled: "we are still checking the Health Value every frame", "this setup can lead to poor performance"; Epic's guidance became "use events whenever possible instead of Bindings or Tick events". The replacement, UMG Viewmodel, is "a more efficient alternative to raw attribute binding, as it only updates widgets when you update the variables": bindings with declared sources (`FieldNotify`), run once "between the PreConstruct and the Construct event", with setters that "check to see if the value changed before assigning the value and then broadcasting". Epic did not remove bindings; it made them change-driven with declared reads and compare-before-write. That is C's shape.
- **Bevy.** The UI vision document: "Bevy's ECS already ships with a powerful tool for incrementalization: change detection!", "we will not be shipping existing Rust reactivity frameworks as part of `bevy_ui`", and "`bevy_ui` uses the same data structures and tools for logic as the rest of your game". cart's scene/UI proposal: the strategy "should use fine grained observer-style reactivity", the earlier prototype's "reactive system is driven by ECS change detection", "Reactive BSN is still largely an open question", and for exposing ECS data to reactive scenes he is "partial to something similar to system syntax". `bevy_ui_widgets` 0.20 uses "external state management" and names React's controlled widgets as the model. The ecosystem's signal frameworks sit on change detection (Quill, bevy_cobweb) and churn (`bevy_cobweb_ui` unmaintained since 2026-01-13). Bevy's own direction is C: reactivity is change detection, the same for UI and scene, and a reactive scene expression is a system in disguise.

Across all six: nobody puts bindings on scene objects except r3f, whose rulebook is about ownership and per-frame state; everybody's UI bindings are change-driven with declared sources; the one vendor who shipped polled bindings steered away from polling, not from bindings.

## The five things, answered under C

**Ownership.** A bound field has one writer, the site. The reconciler does not touch it (provenance `bound`, decision 10). A TS system that declares `write` on the component is caught at load, with both locations in the warning, because TS systems declare their writes (bridge decision 5) and a site's write is known from the prop. A Rust system cannot be caught; Bevy's own ownership is convention there too. Each site runs with its own tick (bridge decision 6 gives every TS system run one), so "who wrote this last" is answerable from the row.

**Hot reload.** Closures die with module state; sites are rebuilt by the generation like systems and `useComponent` subscriptions (tsx hot-reload rule 10). A bound value equal to the row's value writes nothing. Changing a binding's expression re-evaluates every instance once, at the cost of a system over N rows (27 µs at 10k, 0.8 ms at 100k), not a remount of N effects (42 ms at 100k). Turning a bound prop into a literal flips its provenance to `controlled` and writes once; the reverse registers a site.

**Levels and the viewport.** The ruling stands: a computed prop is read-only in its TSX, and the viewport shows a bound prop with its expression and site location, as UMG's Designer shows a bound property's function and Unity's UI Builder shows a binding badge. You cannot drag a bound `intensity`. A `.bsn` level override of a bound field is the `levels` change's call; the recommendation is Godot's rule, an override pins that instance (it leaves the site) and shows a revert, over rejecting the override. Either keeps one owner per instance.

**Debuggability.** A site is one function with a source location, a read set and a write, shown by `bevy-ts/tsx_node`; a breakpoint in it fires per changed row. Decision 9's per-instance effects are N closures sharing one host call's tick, so a patch cannot say which instance's effect produced it. A site runs in `PostUpdate` after TS systems, so a system in `Update` that reads `PointLight` sees last frame's value: one frame of latency, the same as decision 9 and as a system ordered after it.

**Agents writing game code.** Agents reproduce the corpus's idiom, and the React corpus's idiom is props-as-bindings; the r3f pitfalls page exists because humans do the same. An agent cannot judge how many instances a prefab will have, so a feature with a per-instance cost cliff (B) will be used past the cliff. A rule that lives in prose ("bindings in UI only") is forgotten; a rule that lives in `tsc` and load errors is followed. C has no cliff, and its one-owner rule is a load-time message with two locations. A's rule is also enforceable, at the price of making the most readable form illegal and teaching a UI/scene line that a world-space health bar straddles.

## Numbers

Model: N entities, each `Alarm { on, level }` and `Light { intensity, range, radius, pad }` plus 112 bytes of ballast, in one table; k alarms changed this frame. JS numbers are the host call's body in V8 (crossing not included); Rust numbers are criterion medians in a real `bevy_ecs` 0.20 world. Microseconds per frame.

Mounting N per-instance effects (option B):

| N | mount ms | heap bytes per instance |
|---|---|---|
| 1,000 | 0.9 | 677 |
| 10,000 | 1.7 | 655 |
| 100,000 | 42.5 | 656 |

Under C a site is one closure whatever N is; the per-instance data is the ECS row that already exists.

Per frame:

| N | changed k | B: effects fire + patch ops, JS (snapshot / typed slot) | B: Rust finds the k, every frame (`tick_scan` N) | B: Rust applies k patches | C floor: Bevy `Changed` query, Rust | C ceiling: TS loop in JS, scan N write k |
|---|---|---|---|---|---|---|
| 1,000 | 1 | 0.20 / 0.18 | 3.2 | 0.01 | 0.41 | 1.9 |
| 1,000 | 1,000 | 357 / 201 | 3.2 | 25.6 | 1.9 | 3.5 |
| 10,000 | 100 | 19.0 / 19.5 | 64.0 | 1.9 | 8.1 | 9.8 |
| 10,000 | 1,000 | 162 / 184 | 64.0 | 25.4 | 7.1 | 14.5 |
| 10,000 | 10,000 | 4,549 / 8,160 | 64.0 | 216 | 13.0 | 61.7 |
| 100,000 | 1,000 | 314 / 893 | 360 (839 in random order) | 11.8 | 43.3 | 101 |
| 100,000 | 10,000 | 11,866 / 19,708 | 360 | 219 | 36.4 | 193 |
| 100,000 | 100,000 | 280,043 / 205,189 | 360 | 1,440 | 340 | 926 |

How to read it:

- A HUD is the first row: tens to a thousand subscriptions, a few changing. Everything is under 5 µs. Decision 9's per-pair path is right for UI and stays.
- B's three columns add up. At 10k instances with 1% changing, B is about 85 µs against C's 10 to 18; at 100k with 10% changing, 12.4 ms against 0.23 ms; with every instance changing, 282 ms against 1.3 ms. B's JS cost per fired effect grows from 0.2 µs at small N to 2.8 µs at 100k (dependency re-tracking through `Set`s and garbage per snapshot); the typed-slot variant removes the garbage and is still 2 µs.
- B pays its `tick_scan` column on every frame, quiet or not: 64 µs at 10k, 360 to 839 µs at 100k, against 8 and 32 µs for a column-order `Changed` scan. "Zero JS calls in a quiet frame" holds for both; "zero cost" holds only for C.
- B's patch application is random access through a node-id map (1.4 ms for 100k ops); C writes through the column view in the loop, which `system-all` prices at 27 to 800 µs for every row.
- B writes nothing when the value is unchanged (a run of 10k/10% with the same value costs 191 µs and emits no op). C must keep that by comparing before writing; `docs/research/change-detection.md` prices the compare at 2× the plain store (6.1 against 3.1 ms per 1M rows), which is why it is a rule for derived fields and not for systems.
- The per-site Rust check is not measured as such. It sits between the `Changed` query (same-entity reads, table order) and `tick_scan` (cross-entity reads through refs, per-pair lookups).

## Decision

C. Bindings are allowed on scene elements as derived fields with one owner. Those on native-layout fields run per site through the system path; those on reflected components keep decision 9. The IDE shows bound props read-only; `.bsn` overrides of bound fields are settled in `levels`, Godot's pin-and-revert recommended.

What it beat:

- **A. UI only.** Simplest rule, and enforceable. But the line is not where the problems are: a `<text>` binding and a `PointLight` binding have the same ownership question and the same cost curve, and a world-space label is on both sides of the line. It makes the most readable form of "this field follows that one" illegal to avoid a cost cliff that C removes.
- **B. Solid effects everywhere (the draft).** Right for UI, wrong by 50 to 200× at prefab scale, with a per-frame tax proportional to subscriptions and 66 MB of heap at 100k instances. Its two-writer rule is a dev warning that only fires once the fight is happening.

## Changes made to `openspec/changes/archive/2026-10-21-tsx`

- `design.md`: decision 11 added under Decisions, marked a recommendation pending the operator: scene bindings are derived fields with one owner, grouped by site, run as a generated system over changed instances with fixed reads and compare-before-write; reflected targets keep decision 9. Decisions 3 and 9 point at it. The data model names the site table (JS) and site instances (Rust). The two-writer risk now names both locations and the per-site tick; a fixed-reads risk is added. The first open question becomes "the operator confirms decision 11".
- `tasks.md`: 5.6 names both locations in the two-writer warning; 5.7 added: sites, the Rust check, per-row evaluation in one host call, compare-before-write, the dev error for a new read, and a test that 10,000 bound instances with 100 changed cost within 2× the equivalent TS system.
- `specs/reactive-ui/spec.md`: requirement "Bindings on native-layout fields are derived fields with one owner", with scenarios for 10,000 bound lights, a quiet frame and a same-value evaluation.

If the operator rules A instead: delete decision 11, task 5.7 and the requirement; make a function-valued prop on a component element a `tsc` error and a load error naming the element; keep bindings on `<text>`, `<node>`, `<image>`, `<button>` props and children; decision 9 stays as is.

## Sources

- react-three-fiber, Performance pitfalls: https://r3f.docs.pmnd.rs/advanced/pitfalls
- Solid, Intro to reactivity: https://docs.solidjs.com/concepts/intro-to-reactivity
- Godot: Instancing https://docs.godotengine.org/en/stable/getting_started/step_by_step/instancing.html ; Running code in the editor https://docs.godotengine.org/en/stable/tutorials/plugins/running_code_in_the_editor.html ; Scene organization https://docs.godotengine.org/en/stable/tutorials/best_practices/scene_organization.html ; DataBind Controls https://github.com/jamie-pate/godot-control-data-binds
- Unity 6 UI Toolkit: Runtime data binding https://docs.unity3d.com/6000.0/Documentation/Manual/UIE-runtime-binding.html ; Define a data source https://docs.unity3d.com/Manual/UIE-runtime-binding-define-data-source.html ; Binding modes and update triggers https://docs.unity3d.com/6000.0/Documentation/Manual/UIE-runtime-binding-mode-update.html
- Unreal Engine: Driving UI updates with events https://dev.epicgames.com/documentation/en-us/unreal-engine/driving-ui-updates-with-events-in-unreal-engine ; UMG best practices https://dev.epicgames.com/documentation/en-us/unreal-engine/umg-best-practices-in-unreal-engine ; UMG Viewmodel https://dev.epicgames.com/documentation/en-us/unreal-engine/umg-viewmodel-for-unreal-engine ; Property binding https://dev.epicgames.com/documentation/en-us/unreal-engine/property-binding-for-umg-in-unreal-engine
- Bevy: UI vision document https://hackmd.io/@bevy/HkjcMkJFC ; cart, Next Generation Scene / UI https://github.com/bevyengine/bevy/discussions/14437 ; `bevy_ui_widgets` 0.20 `src/lib.rs` (`~/dev/refs/bevy-0.20`); `bevy_cobweb` and `bevy_cobweb_ui` READMEs (`~/dev/refs`)
- bevy-ts: `openspec/changes/archive/2026-10-21-tsx/design.md` (decisions 3, 5, 9, 10, operator rulings), `openspec/changes/bridge/design.md` (decisions 5, 6), `docs/research/change-detection.md`, `bench/bindings/`
