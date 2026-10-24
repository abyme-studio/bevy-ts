// The <For> bench: one keyed list of n rows, and what one change to it costs.
// run.mjs writes src/config.ts and reads the `bench:` line this prints.
import { app, Component, For, Last, signal, Update, type f32 } from "bevy-ts";
import type { Commands, Query } from "bevy-ts";
import { Transform } from "bevy-ts/bevy";
import { C } from "./config.ts";

export class Row extends Component {
  i: f32 = 0;
}

const R = ({ i }: { i: number }) => (
  <entity key={i}>
    <Transform translation={[i % 100, 0, Math.floor(i / 100)]} />
    <Row i={i} />
  </entity>
);

const [items, setItems] = signal<number[]>([]);
const range = (a: number, b: number) => Array.from({ length: b - a }, (_, k) => a + k);

app.scene("bench", () => (
  <entity name="list">
    <For each={items} key={(i: number) => i}>
      {(i: number) => <R i={i} />}
    </For>
  </entity>
));

const clock = () => (globalThis as { performance?: { now(): number } }).performance?.now() ?? Date.now();
const START = 30;
let frame = 0;
/** The measured change: the frame it was made and when. */
let at = -1;
let t0 = 0;
let longest = 0;
let last = 0;
let phase: "warm" | "fill" | "settle" | "measure" | "done" = "warm";
let settled = 0;

function report(rows: number) {
  const ms = clock() - t0;
  console.log(`bench: ${C.arm} ${C.n} ${ms.toFixed(1)} ms, ${frame - at + 1} frames, longest ${longest.toFixed(1)} ms, rows ${rows}`);
  phase = "done";
}

export function change(commands: Commands) {
  frame++;
  if (phase === "warm" && frame === START) {
    if (C.arm === "reverse" || C.arm === "clear") {
      setItems(range(0, C.n));
      phase = "fill";
      return;
    }
    phase = "measure";
    at = frame;
    t0 = clock();
    last = t0;
    if (C.arm === "grow") setItems(range(0, C.n));
    if (C.arm === "grow1k") setItems(range(0, Math.min(1000, C.n)));
    if (C.arm === "spawn") for (let i = 0; i < C.n; i++) commands.spawn(<R i={i} />);
    return;
  }
  if (phase === "settle" && ++settled === 10) {
    phase = "measure";
    at = frame;
    t0 = clock();
    last = t0;
    setItems(C.arm === "reverse" ? range(0, C.n).reverse() : []);
    return;
  }
  if (phase === "measure" && C.arm === "grow1k" && items().length < C.n) {
    const k = items().length;
    setItems([...items(), ...range(k, Math.min(k + 1000, C.n))]);
  }
}

export function end(rows: Query<[Row]>) {
  const now = clock();
  if (phase === "fill" && rows.len === C.n) phase = "settle";
  if (phase !== "measure") return;
  longest = Math.max(longest, now - last);
  last = now;
  const want = C.arm === "clear" ? 0 : C.n;
  // A reorder spawns and despawns nothing: its one call is in the frame of the set.
  if (C.arm === "reverse" || rows.len === want) report(rows.len);
}

app.addSystems(Update, change);
app.addSystems(Last, end);
