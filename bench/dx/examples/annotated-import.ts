// A system whose param types live in another file.
import type { Enemies, Scoring } from "./annotated-clean.ts";
export function tally(enemies: Enemies, s: Scoring) {
  enemies.each(() => { s.score.value += 1; });
}
