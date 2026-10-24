import { connect } from "./client.js";

const base = process.env.ABYME_URL ?? "http://127.0.0.1:4477";
const key: string | undefined = process.env.ABYME_KEY;
const url = new URL("/api/stream", base);
url.protocol = "ws:";

// The non-standard, Deno-shaped option; loopback only at run time.
const ws = new WebSocket(url, { headers: { authorization: `Bearer ${key ?? ""}` } });
ws.binaryType = "arraybuffer";
ws.onopen = () => ws.send(JSON.stringify({ hello: true }));
ws.onmessage = (e) => {
  if (typeof e.data === "string") console.log(e.data.length);
};
ws.addEventListener("close", (e) => console.log(e.code, e.reason, e.wasClean));
new WebSocket("ws://127.0.0.1:9001/echo", ["v1", "v2"]).close(1000, "done");

const controller = new AbortController();
controller.signal.addEventListener("abort", () => console.log(controller.signal.reason));
const timeout = AbortSignal.timeout(1000);
timeout.throwIfAborted();
const signal = AbortSignal.any([controller.signal, timeout]);

const t = setTimeout((n: number) => console.log(n), 50, 1);
clearTimeout(t);
clearInterval(setInterval(() => controller.abort(new Error("stop")), 1000));

for (const [k, v] of new URLSearchParams(url.search)) console.log(k, v);
try {
  ws.close(1001);
} catch (e) {
  if (e instanceof DOMException) console.log(e.name);
}
const target = new EventTarget();
target.dispatchEvent(new Event("ping"));
console.log(new CloseEvent("close", { code: 1000 }).code, new MessageEvent("message", { data: 1 }).data);

void connect(url.href, { signal });

const elapsed: number = performance.now() - performance.timeOrigin;
console.log(elapsed);
