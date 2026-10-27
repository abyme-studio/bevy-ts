// The Relationships example in docs/scripts.md, compiled, and a target's row type.
import { Component, Relationship, RelationshipTarget, f32 } from "bevy-ts";
import type { Commands, Entity, Option, Query, TargetList, With } from "bevy-ts";

export class Fan extends Component {}
export class Idol extends Component {}
export class Likes extends Relationship { declare target: Entity; since: f32 = 0 }
export class LikedBy extends RelationshipTarget(Likes) {}

export function like(c: Commands, fans: Query<[Entity], With<Fan>>, idols: Query<[Entity], With<Idol>>) {
  idols.each((idol) => fans.each((fan) => c.entity(fan).insert(Likes.of({ target: idol }))));
}
export function fans(q: Query<[Entity, LikedBy]>) {
  q.each((e, by) => console.log(`${e} is liked by ${by.length}: ${by.toArray()}`));
}
export function maybe(q: Query<[Option<LikedBy>]>) {
  q.each((by) => {
    const list: TargetList | undefined = by;
    const first: Entity | undefined = list?.at(0);
    for (const e of list ?? []) void (e satisfies Entity);
    void first;
  });
}
