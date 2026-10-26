// Type tests for heap fields on TS components (ts-component-heap-fields): a `string` field reads as
// a string and a `T[]` as `readonly T[]`, assigned whole on a `Mut` row, read-only on a read row. `tsc -p test` must
// pass, and every `@ts-expect-error` must be an error.
import { Component, type f32, type Mut, type Query, type u32 } from "bevy-ts";

export class Tag extends Component {
  label: string = "crate";
  hp: f32 = 0;
  scores: u32[] = [];
}

export function rename(q: Query<[Mut<Tag>]>) {
  q.each((t) => {
    if (t.label === "door") t.label = "open";
    t.scores = [...t.scores, 1];
    // @ts-expect-error a list is assigned whole, never edited in place
    t.scores.push(2);
  });
}

export function look(q: Query<[Tag]>) {
  q.each((t) => {
    const s: string = t.label;
    const n: number = t.scores[0];
    // @ts-expect-error a read row is read-only
    t.label = "x";
    // @ts-expect-error a list is never edited in place
    t.scores.push(3);
    void [s, n];
  });
}

export const spawned = Tag.of({ label: "lamp", scores: [1, 2] });
