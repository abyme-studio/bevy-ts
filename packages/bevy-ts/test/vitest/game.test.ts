import { expect, test } from "vitest";
import { declarationsOf, load, rows } from "bevy-ts/testing";

test("a system compiled by the plugin declares its params and runs on hand-built rows", async () => {
  const manifest = await load(() => import("./game.ts"));
  const { speedUp } = await import("./game.ts");
  expect(manifest.systems.map((s) => s.name)).toEqual(["vitest/game.ts#speedUp"]);
  expect(manifest.components.map((c) => c.name)).toEqual(["game::Mover"]);
  const decl = declarationsOf(speedUp);
  expect(decl?.params?.[0].kind).toBe("query");
  const mover = { speed: 2 };
  speedUp(rows([[mover]]) as never);
  expect(mover.speed).toBe(4);
});
