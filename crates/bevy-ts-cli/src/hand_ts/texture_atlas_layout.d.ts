/** A grid of equal tiles, Bevy's `TextureAtlasLayout::from_grid`: sizes in whole pixels. */
export interface TextureAtlasGrid {
  /** One tile's `[width, height]`. */
  tileSize: readonly [number, number];
  columns: number;
  rows: number;
  /** The gap between tiles, `[x, y]` (default none). */
  padding?: readonly [number, number];
  /** Where the first tile starts, `[x, y]` from the top left (default `[0, 0]`). */
  offset?: readonly [number, number];
}
/** Bevy's `TextureAtlasLayout`; `fromGrid` makes one from a grid. */
export declare const TextureAtlasLayout: AssetType<TextureAtlasLayout> & {
  /** A layout of `columns` by `rows` tiles: equal grids share one layout. */
  fromGrid(grid: TextureAtlasGrid): AssetValue<TextureAtlasLayout>;
};
