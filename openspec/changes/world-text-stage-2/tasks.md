## 1. Text along a path

- [ ] 1.1 `WorldTextStyle` gains `path: Vec<Vec3>` and `normal: Vec3`; `ui.rs` takes `path` (at least two points) and `normal`, refuses `path` with `width` or a line break, naming the props; stage 1's refusal of `path` goes
- [ ] 1.2 `world_text/mesh.rs`: each glyph's centre advance placed on the polyline by arc length, its quad turned to the tangent and `normal`; `justify` start, centre, end along the path; past the end along the last tangent (archived design, decision 9)
- [ ] 1.3 `world_text.rs` test: `ROME` on a 64-point half circle of radius 4, centre placement and tangent within the spec's bounds; `path` with `width` fails; `jsx-runtime.d.ts` and `elements.tsx` gain `path` and `normal`
- [ ] 1.4 `docs/world-text.md`: the path section; `examples/cube`: a curved label
