// Type tests for heap fields on TS components (ts-component-heap-fields): a `string` field reads as
// a string and a `T[]` as `readonly T[]` on a read row; on a `Mut` row a list is an array edited in
// place or assigned whole (ts-heap-fields-2 stage 5). `tsc -p test` must
// pass, and every `@ts-expect-error` must be an error.
import { Component, type Entity, type f32, type Mut, type Query, type u32, vec3, type Vec3 } from "bevy-ts";

export class Tag extends Component {
  label: string = "crate";
  hp: f32 = 0;
  scores: u32[] = [];
}

export function rename(q: Query<[Mut<Tag>]>) {
  q.each((t) => {
    if (t.label === "door") t.label = "open";
    t.scores = [...t.scores, 1];
    t.scores.push(2);
    t.scores[0] = 3;
    t.scores.length = 1;
  });
}

export function look(q: Query<[Tag]>) {
  q.each((t) => {
    const s: string = t.label;
    const n: number = t.scores[0];
    // @ts-expect-error a read row is read-only
    t.label = "x";
    // @ts-expect-error a list on a read row is never edited in place
    t.scores.push(3);
    void [s, n];
  });
}

export const spawned = Tag.of({ label: "lamp", scores: [1, 2] });

// Richer list items (ts-heap-fields-2 stage 4): vectors read as read-only vectors, objects as
// read-only objects, entities as `Entity`, unions as their names.
type Kind = "hit" | "miss";
export class Trail extends Component {
  points: Vec3[] = [];
  hits: { at: Vec3; by: Entity; kind: Kind; hp: f32 }[] = [];
  who: Entity[] = [];
}

export function walk(q: Query<[Mut<Trail>]>) {
  q.each((t) => {
    const p = t.points[0];
    const len: number = p.length();
    // @ts-expect-error a list's vector item is read-only
    p.x = 1;
    const h = t.hits[0];
    const k: Kind = h.kind;
    const by: Entity = h.by;
    // @ts-expect-error a list's object item is read-only
    h.hp = 2;
    // @ts-expect-error its vector member too
    h.at.x = 3;
    t.points = [...t.points, vec3(1, 2, 3)];
    t.points.push(vec3(4, 5, 6));
    t.hits[0] = { ...h, hp: 2 };
    t.hits = [{ at: vec3(), by: t.who[0], kind: "hit", hp: 1 }];
    void [len, k, by];
  });
}
