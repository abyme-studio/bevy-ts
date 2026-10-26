# material-params 4.2: owned writes against re-keying

N cubes, each with its own `game::Glow`, its `pulse` changed every frame (60 Hz is the target;
the bench runs frames back to back). Two arms, run alternately:

- `set`: N `Glow.add` instances, `assets.set(g, { pulse })` on each every frame. The asset stays,
  written in place at its `AssetId`.
- `rekey`: N `<material kind={Glow} pulse={...} />` rows of a `<For>` keyed by their pulse, the
  only way to change a material prop from TSX before this change. Every row respawns with a new
  keyed instance, and the old one is swept.

Each run: the full engine (V8, TSX, ScriptMaterialPlugin, Bevy's renderer), offscreen 640x360,
pipelined rendering off, 120 warm-up frames, then 600 measured. Pulse never repeats, so the keyed
cache never holds a row's next value already.

```
bench/material-params/run.sh 3      # the material-params-bench process
```

Apple M5 Max, 18 cores, 128 GB, macOS 26.4, release, 2026-10-26. Other agents' builds ran
meanwhile: load average 14 at the start, 99 at the end. 3 runs per arm; the arms' ranges do not
overlap at any N, so no 8-run round.

| N | arm | frame mean ms (median) | range | p95 ms | assets at most | prepares per frame |
|---|-----|---:|---|---:|---:|---:|
| 1 | set | 9.7 | 8.0–10.6 | 23.9 | 1 | 1.6 |
| 1 | rekey | 11.8 | 10.7–12.2 | 25.9 | 2 | 1.5 |
| 10 | set | 10.9 | 10.8–11.8 | 26.1 | 10 | 14.9 |
| 10 | rekey | 13.7 | 13.4–15.8 | 35.5 | 20 | 13.3 |
| 100 | set | 24.6 | 22.5–27.7 | 49.9 | 100 | 105.8 |
| 100 | rekey | 37.7 | 31.2–46.2 | 97.0 | 200 | 100.2 |

"Prepares per frame" counts `ScriptMaterial` asset events, Added or Modified; each one prepares
the asset again in the render world (a uniform buffer and a bind group). Both arms prepare about
one per instance per frame. Render pipelines stayed at 44 in every run of both arms: neither
builds a pipeline per write.

What it says:

- `assets.set` is cheaper at every N, and the gap grows with N: 35% less frame time at 100.
  Re-keying pays for respawning entities and making and sweeping assets, not for the prepare.
- Re-keying holds twice the assets (the new one and the one waiting for the sweep).
- Both still prepare every changed instance every frame. Writing N instances at 60 Hz costs N
  bind groups a frame either way.

`runs.jsonl` holds every run. `runs-cycling-pulse.jsonl` is an earlier round whose pulse repeated
every 100 frames, so the re-key arm found its values in the keyed cache: its numbers do not
measure re-keying.

# material-params 4.3: instance params against re-keying

10,000 cubes on a 100x100 grid, one `pulse`-like value per cube changing every frame. Two arms,
run alternately:

- `instance`: `<material kind={Hover} color="#ff8800" hover={() => wave(i, t())} />`. `hover` is
  an instance param: every cube shares one asset, and the bound value writes the cube's
  `ScriptInstance`, packed into one storage buffer.
- `instance-rekey`: the same wave as a `Glow` param, through a `<For>` keyed by the value, as
  before this change. Every row respawns with a new keyed instance each frame.

Same engine and target as 4.2; 30 warm-up frames, then 150 measured (a re-keyed frame takes
seconds at this N).

```
bench/material-params/run.sh 3 3    # the material-params-bench-instance process
```

Apple M5 Max, release, 2026-10-26, on bevy-ts's own packing (rows written with the component,
uploaded once a frame). Other agents' builds ran meanwhile: load average 100 at the start, 35 at
the end, so the spread within an arm is wide. 3 runs per arm; the arms' ranges are more than 14x
apart, so no 8-run round.

| arm | frame mean ms (median) | range | p95 ms (median) | assets at most | prepares per frame | instance buffer bytes |
|-----|---:|---|---:|---:|---:|---:|
| instance | 61.1 | 25.5–83.6 | 111.5 | 1 | 0 | 2,560,000 |
| instance-rekey | 2009.6 | 1208.8–2628.2 | 4786.1 | 20,000 | 10,000 | 256 |

What it says:

- Instance params are about 33x cheaper per frame at 10k. No asset is prepared: the change is one
  buffer upload.
- The buffer is 256 bytes a cube, 2.56 MB for 10,000, uploaded whole once a frame when any value
  changed. The re-key arm's kind declares no instance params; its 256 bytes are the empty
  buffer's one row.
- Re-keying prepares every cube's asset every frame and holds twice the assets.
- Render pipelines stayed at 44 in every run of both arms.

`runs-instance.jsonl` holds every run. `runs-instance-bevy-packing.jsonl` is the round before,
on Bevy's `GpuComponentArrayBuffer` (load 60 to 21): instance 48.5 ms median, re-key 1812.8 ms,
buffer 2.87 MB. Bevy's packing was replaced because it could not be ordered against the writes.
