//! `TextureAtlasLayout` as asset data: its hand-mapped entry (change script-2d, design 2). Bevy
//! gives it no reflected default, so `T.of` cannot build it; this entry takes Bevy's
//! `TextureAtlasLayout::from_grid` instead, and its own fields:
//!
//! ```json
//! { "fromGrid": { "tileSize": [32, 32], "columns": 8, "rows": 4, "padding": [0, 0], "offset": [0, 0] } }
//! { "size": [256, 128], "textures": [{ "min": [0, 0], "max": [32, 32] }] }
//! ```
//!
//! `TextureAtlasLayout.fromGrid({...})` in TS is the first as an asset value, so every slot given
//! an equal grid shares one layout: a sprite's `texture_atlas.layout` and `<image atlas>`'s.

use crate::reflect_asset::Slots;
use bevy::image::TextureAtlasLayout;
use bevy::math::{URect, UVec2};
use serde_json::{Map, Value, json};

/// The asset's type path, as a `fromGrid` value's `$of` names it.
pub const PATH: &str = "bevy_image::texture_atlas::TextureAtlasLayout";

const GRID_KEYS: &[&str] = &["tileSize", "columns", "rows", "padding", "offset"];

/// A grid, as `TextureAtlasLayout::from_grid` takes it.
struct Grid {
    tile_size: UVec2,
    columns: u32,
    rows: u32,
    padding: UVec2,
    offset: UVec2,
}

fn pair(v: &Value, what: &str, positive: bool) -> Result<UVec2, String> {
    let ok = |n: &Value| n.as_u64().filter(|n| *n <= u32::MAX as u64 && (!positive || *n > 0)).map(|n| n as u32);
    let kind = if positive { "positive whole pixels" } else { "whole pixels" };
    match v.as_array().map(Vec::as_slice) {
        Some([x, y]) => Ok(UVec2::new(ok(x).ok_or_else(|| format!("{what} takes [x, y] in {kind}, not {v}"))?, ok(y).ok_or_else(|| format!("{what} takes [x, y] in {kind}, not {v}"))?)),
        _ => Err(format!("{what} takes [x, y] in {kind}, not {v}")),
    }
}

fn grid(v: &Value) -> Result<Grid, String> {
    let o = v.as_object().ok_or_else(|| format!("fromGrid takes {{ tileSize, columns, rows, padding?, offset? }}, not {v}"))?;
    if let Some(k) = o.keys().find(|k| !GRID_KEYS.contains(&k.as_str())) {
        return Err(format!("fromGrid has no {k} (it takes {})", GRID_KEYS.join(", ")));
    }
    let count = |k: &str| {
        let n = o.get(k).ok_or_else(|| format!("fromGrid needs {k}"))?;
        n.as_u64().filter(|n| *n > 0 && *n <= u32::MAX as u64).map(|n| n as u32).ok_or_else(|| format!("fromGrid.{k} takes a positive whole number, not {n}"))
    };
    let tile_size = pair(o.get("tileSize").ok_or("fromGrid needs tileSize: [width, height]")?, "fromGrid.tileSize", true)?;
    let (columns, rows) = (count("columns")?, count("rows")?);
    let padding = o.get("padding").map(|p| pair(p, "fromGrid.padding", false)).transpose()?.unwrap_or(UVec2::ZERO);
    let offset = o.get("offset").map(|p| pair(p, "fromGrid.offset", false)).transpose()?.unwrap_or(UVec2::ZERO);
    columns.checked_mul(rows).ok_or("fromGrid: columns * rows is too many tiles")?;
    Ok(Grid { tile_size, columns, rows, padding, offset })
}

/// The number of tiles in a `Handle<TextureAtlasLayout>` slot's JSON when it is a `fromGrid`
/// value, or its grid's error; `None` for a path, a ref or any other value.
pub fn tiles(slot: &Value) -> Option<Result<usize, String>> {
    let o = slot.as_object()?;
    if o.get("$of")?.as_str()? != PATH {
        return None;
    }
    let g = o.get("props")?.get("fromGrid")?;
    Some(grid(g).map(|g| g.columns as usize * g.rows as usize))
}

/// `texture_atlas.index` checked against its layout when the layout is a `fromGrid` value: Bevy
/// would draw nothing for a tile past the end.
pub fn check_index(atlas: &Value, what: &str) -> Result<(), String> {
    let Some(o) = atlas.as_object() else { return Ok(()) };
    let Some(n) = o.get("layout").and_then(tiles) else { return Ok(()) };
    let n = n.map_err(|e| format!("{what}.layout: {e}"))?;
    let Some(index) = o.get("index").and_then(Value::as_u64) else { return Ok(()) };
    if index as usize >= n {
        return Err(format!("{what}.index {index} is past the layout's {n} tiles"));
    }
    Ok(())
}

fn uvec2(v: UVec2) -> Value {
    json!([v.x, v.y])
}

/// The hand-map read: the JSON it was written from when there is one, else Bevy's fields.
pub fn read(_: &Slots, source: Option<&Value>, layout: &TextureAtlasLayout) -> Result<Value, String> {
    if let Some(s) = source {
        return Ok(s.clone());
    }
    let textures: Vec<Value> = layout.textures.iter().map(|r| json!({ "min": uvec2(r.min), "max": uvec2(r.max) })).collect();
    Ok(json!({ "size": uvec2(layout.size), "textures": textures }))
}

/// The hand-map write: `{ fromGrid }`, or `{ size, textures }` over the base's (an empty layout's).
pub fn write(_: &Slots, base: Option<&TextureAtlasLayout>, json: &Value) -> Result<TextureAtlasLayout, String> {
    let o: &Map<String, Value> = json.as_object().ok_or_else(|| format!("a TextureAtlasLayout takes {{ fromGrid }} or {{ size, textures }}, not {json}"))?;
    if let Some(g) = o.get("fromGrid") {
        if let Some(k) = o.keys().find(|k| *k != "fromGrid") {
            return Err(format!("a TextureAtlasLayout from fromGrid takes no {k}"));
        }
        let g = grid(g)?;
        return Ok(TextureAtlasLayout::from_grid(g.tile_size, g.columns, g.rows, Some(g.padding), Some(g.offset)));
    }
    let mut layout = base.cloned().unwrap_or_else(|| TextureAtlasLayout::new_empty(UVec2::ZERO));
    for (k, v) in o {
        match k.as_str() {
            "size" => layout.size = pair(v, "size", false)?,
            "textures" => {
                let a = v.as_array().ok_or_else(|| format!("textures takes [{{ min, max }}], not {v}"))?;
                layout.textures = a
                    .iter()
                    .map(|r| Ok(URect::from_corners(pair(r.get("min").unwrap_or(&Value::Null), "textures[].min", false)?, pair(r.get("max").unwrap_or(&Value::Null), "textures[].max", false)?)))
                    .collect::<Result<_, String>>()?;
            }
            _ => return Err(format!("TextureAtlasLayout has no field {k} (it takes fromGrid, or size and textures)")),
        }
    }
    Ok(layout)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(grid: Value) -> Value {
        json!({ "$of": PATH, "props": { "fromGrid": grid } })
    }

    #[test]
    fn a_grid_counts_its_tiles_and_an_index_past_them_names_both() {
        let v = value(json!({ "tileSize": [32, 32], "columns": 8, "rows": 4 }));
        assert_eq!(tiles(&v), Some(Ok(32)));
        assert_eq!(tiles(&json!("atlas.ron")), None);
        let bad = check_index(&json!({ "layout": value(json!({ "tileSize": [32, 32], "columns": 0, "rows": 1 })), "index": 0 }), "atlas").unwrap_err();
        assert!(bad.contains("atlas.layout: fromGrid.columns"), "{bad}");
        check_index(&json!({ "layout": v, "index": 31 }), "texture_atlas").unwrap();
        let e = check_index(&json!({ "layout": v, "index": 32 }), "texture_atlas").unwrap_err();
        assert!(e.contains("texture_atlas.index 32") && e.contains("32 tiles"), "{e}");
    }

    #[test]
    fn a_bad_grid_names_its_prop() {
        for (g, prop) in [
            (json!({ "columns": 2, "rows": 2 }), "tileSize"),
            (json!({ "tileSize": [0, 32], "columns": 2, "rows": 2 }), "fromGrid.tileSize"),
            (json!({ "tileSize": [32, 32], "columns": 0, "rows": 2 }), "fromGrid.columns"),
            (json!({ "tileSize": [32, 32], "columns": 2, "rows": -1 }), "fromGrid.rows"),
            (json!({ "tileSize": [32, 32], "columns": 2, "rows": 2, "padding": [1] }), "fromGrid.padding"),
            (json!({ "tile": [32, 32], "columns": 2, "rows": 2 }), "tile"),
        ] {
            let e = grid(&g).err().unwrap_or_else(|| panic!("{g} is refused"));
            assert!(e.contains(prop), "{g}: {e} names {prop}");
        }
    }
}
