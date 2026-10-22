# ui-gallery

Every bevy_ui element bevy-ts reaches from TSX, one panel per group: `bevy-ts dev src/gallery.tsx`.
It does not ship with the npm package. `crates/bevy-ts-cli/tests/ui_gallery.rs` loads it headless and
fails when an intrinsic typed in `jsx-runtime.d.ts` is missing from it, so a new element lands with its panel.
