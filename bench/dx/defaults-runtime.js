// How the loader reads default params: call once in declare mode; declarations record,
// any use throws a sentinel the loader catches. Every frame after, it passes real values.
const DECLARED = Symbol("declared");
let mode = "declare", declared = [];
const sentinel = () => new Proxy({}, { get() { throw DECLARED; } });
function query(...d) { if (mode === "declare") { declared.push(["query", d]); return Object.assign(sentinel(), {}); } }
function res(r) { if (mode === "declare") { declared.push(["res", r]); return sentinel(); } }
function system(f) {
  try { f(); } catch (e) { if (e !== DECLARED) throw e; }
  const params = declared; declared = [];
  return { f, params };
}
let bodyRuns = 0;
const chase = system((time = res("Time"), enemies = query("Enemy", "Mut(Transform)")) => {
  bodyRuns++;
  enemies.each(() => {});
});
console.log("declared:", JSON.stringify(chase.params), "body entered in declare mode:", bodyRuns);
// Frame: real values passed, defaults never evaluate.
mode = "run";
let n = 0;
chase.f({ delta: 0.016 }, { each: (cb) => { n++; cb(); } });
console.log("frame ran, each called:", n, "declarations after frame:", declared.length);
