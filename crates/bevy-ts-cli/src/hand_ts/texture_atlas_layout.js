/**
 * Bevy's `TextureAtlasLayout`, and `fromGrid(grid)`: Bevy's `TextureAtlasLayout::from_grid` as an
 * asset value, so every `Handle<TextureAtlasLayout>` slot given an equal grid shares one layout
 * (a sprite's `texture_atlas.layout`, `<image atlas>`'s `layout`).
 */
export const TextureAtlasLayout = Object.assign(asset("bevy_image::texture_atlas::TextureAtlasLayout", { of: false }), {
  fromGrid: (grid) => Object.freeze({ $of: "bevy_image::texture_atlas::TextureAtlasLayout", props: Object.freeze({ fromGrid: grid }) }),
});
