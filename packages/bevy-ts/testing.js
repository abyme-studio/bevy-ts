// `bevy-ts/testing`: load game modules outside the engine and call their systems with hand-built
// params. Modules must be compiled by bevy-ts (its Vite/vitest or esbuild plugin) so their systems
// carry declarations.
import "bevy-ts";

const DECL = Symbol.for("bevy-ts.decl");

let next = 1;

/**
 * Loads modules as a generation: `await load(() => import("./game.ts"))`. Returns the manifest the
 * engine would receive (systems with identities and params, components, resources, ...).
 */
export async function load(importer) {
  const host = globalThis.__bevyts;
  const gen = next++;
  host.begin(gen);
  try {
    await importer();
  } catch (e) {
    host.abort();
    throw e;
  }
  const [json] = host.end();
  return JSON.parse(json);
}

/** The declarations the compile step attached to a system function, or undefined. */
export function declarationsOf(fn) {
  return fn ? fn[DECL] : undefined;
}

/** A stand-in query over rows you build: `each`, iteration, `single`, `get`, `len`. */
export function rows(list, entities) {
  const ents = entities ?? list.map((_, i) => i);
  return {
    each(fn) {
      for (const r of list) fn(...r);
    },
    *[Symbol.iterator]() {
      yield* list;
    },
    single() {
      if (list.length !== 1) throw new Error(`single(): expected exactly one match, found ${list.length}`);
      return list[0];
    },
    get(e) {
      const i = ents.indexOf(e);
      return i < 0 ? undefined : list[i];
    },
    get len() {
      return list.length;
    },
    tick: 0,
    tables: [],
  };
}
