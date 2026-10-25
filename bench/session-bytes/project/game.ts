// The session-bytes bench: what each SessionBytes call costs, and what 10k stored keys cost a frame.
// Each arm logs one `bench: {json}` line; `bench: done` ends the run (bench.mjs reads them).
import { app, First, Last, Update } from "bevy-ts";
import { get, type SessionBytes } from "bevy-ts/session";

const WARM = 60;
const WINDOW = 300;
const CALLS = 1000;
const KEYS = 10_000;
const FRAMES_10K = 10;

const big = new Uint8Array(64 * 1024).fill(7);
const into = new Uint8Array(64 * 1024);
const small = new Uint8Array(64).fill(3);
const smallInto = new Uint8Array(64);
const keys = Array.from({ length: KEYS }, (_, i) => `id/${i}`);

const quantile = (xs: number[], q: number) => {
  const s = [...xs].sort((a, b) => a - b);
  return s[Math.min(s.length - 1, Math.floor(q * s.length))];
};
const mean = (xs: number[]) => xs.reduce((a, b) => a + b, 0) / xs.length;
const report = (arm: string, unit: string, xs: number[], extra: object = {}) =>
  console.log(`bench: ${JSON.stringify({ arm, unit, n: xs.length, median: quantile(xs, 0.5), p95: quantile(xs, 0.95), mean: mean(xs), ...extra })}`);

/** One call `CALLS` times, each timed (µs); and the whole loop, for a timer coarser than a call. */
function perCall(arm: string, f: () => void) {
  const xs: number[] = [];
  const t0 = performance.now();
  for (let i = 0; i < CALLS; i++) {
    const t = performance.now();
    f();
    xs.push((performance.now() - t) * 1000);
  }
  report(arm, "us", xs, { loopMean: ((performance.now() - t0) * 1000) / CALLS });
}

/** Every key once, in one frame (ms). */
function perFrame(f: (k: string) => void) {
  const t = performance.now();
  for (const k of keys) f(k);
  return performance.now() - t;
}

let frame = 0;
let t0 = 0;
const emptyFrames: number[] = [];
const fullFrames: number[] = [];
const tenK: Record<string, number[]> = { put10k: [], read10k: [], get10k: [] };
// Frames: warm, empty store measured, the 64 KB arms, the 10k arms, warm, full store measured.
const EMPTY_END = WARM + WINDOW;
const ARMS_10K = EMPTY_END + 2;
const FULL_START = ARMS_10K + 3 * FRAMES_10K + WARM;
const FULL_END = FULL_START + WINDOW;

export function start() {
  t0 = performance.now();
}

export function arms(session: SessionBytes) {
  frame++;
  if (frame === EMPTY_END + 1) {
    perCall("put64k", () => session.put("big", big));
    perCall("get64k", () => get(session, "big"));
    perCall("read64k", () => session.read("big", into));
    session.delete("big");
  }
  const k = frame - ARMS_10K;
  if (k >= 0 && k < 3 * FRAMES_10K) {
    const arm = ["put10k", "read10k", "get10k"][k % 3];
    tenK[arm].push(
      perFrame(
        arm === "put10k" ? (key) => session.put(key, small) : arm === "read10k" ? (key) => session.read(key, smallInto) : (key) => get(session, key),
      ),
    );
    if (k === 3 * FRAMES_10K - 1) {
      for (const [a, xs] of Object.entries(tenK)) report(a, "ms", xs, { keys: session.count(), bytes: session.bytes() });
    }
  }
}

export function end() {
  const ms = performance.now() - t0;
  if (frame > WARM && frame <= EMPTY_END) emptyFrames.push(ms);
  if (frame > FULL_START && frame <= FULL_END) fullFrames.push(ms);
  if (frame === FULL_END) {
    report("frame-empty", "ms", emptyFrames);
    report("frame-10k", "ms", fullFrames);
    console.log("bench: done");
  }
}

app.addSystems(First, start);
app.addSystems(Update, arms);
app.addSystems(Last, end);
