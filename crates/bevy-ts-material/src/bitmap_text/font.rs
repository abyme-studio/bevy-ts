//! The `SpriteFont` asset and its loader: XNA's SpriteFont as StardewXnbHack and similar tools
//! export it, a JSON file beside its PNG.

use bevy::asset::io::Reader;
use bevy::asset::{AssetLoader, LoadContext};
use bevy::image::{ImageLoaderSettings, ImageSampler};
use bevy::prelude::*;
use serde::Deserialize;
use std::collections::HashMap;

/// One glyph: its rect in the texture, its cropping (the offset it draws at, and the cell height a
/// line takes), and its bearings, as XNA's `kerning` holds them (x left bearing, y width, z right
/// bearing; XNA has no pair kerning).
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub struct FontGlyph {
    pub bounds: URect,
    pub cropping: IRect,
    pub left: f32,
    pub width: f32,
    pub right: f32,
}

/// An XNA SpriteFont: its texture, glyphs, the character each glyph draws, the line spacing, the
/// spacing between glyphs, and the character drawn for one the font lacks.
#[derive(Asset, Clone, Debug, Reflect)]
pub struct SpriteFont {
    pub texture: Handle<Image>,
    pub glyphs: Vec<FontGlyph>,
    pub characters: Vec<char>,
    #[reflect(ignore)]
    index: HashMap<char, usize>,
    pub line_spacing: f32,
    pub spacing: f32,
    pub default_character: Option<char>,
}

impl SpriteFont {
    /// The glyph `c` draws: its own, else the default character's.
    pub fn glyph_index(&self, c: char) -> Option<usize> {
        self.index.get(&c).or_else(|| self.index.get(&self.default_character?)).copied()
    }

    /// A font from its parts, its texture `texture`; `Err` names the field that does not fit.
    pub fn from_json(json: &[u8], texture: Handle<Image>) -> Result<SpriteFont, String> {
        let f: Json = serde_json::from_slice(json).map_err(|e| e.to_string())?;
        let n = f.glyphs.len();
        for (field, len) in [("cropping", f.cropping.len()), ("characterMap", f.character_map.len()), ("kerning", f.kerning.len())] {
            if len != n {
                return Err(format!("{field} has {len} entries, glyphs {n}"));
            }
        }
        let mut characters = Vec::with_capacity(n);
        for (i, s) in f.character_map.iter().enumerate() {
            let mut chars = s.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => characters.push(c),
                _ => return Err(format!("characterMap[{i}] is {s:?}, not one character")),
            }
        }
        let rect = |r: &JsonRect, field: &str, i: usize| -> Result<IRect, String> {
            if r.width < 0 || r.height < 0 {
                return Err(format!("{field}[{i}] has a negative size"));
            }
            Ok(IRect::new(r.x, r.y, r.x + r.width, r.y + r.height))
        };
        let mut glyphs = Vec::with_capacity(n);
        for i in 0..n {
            let b = rect(&f.glyphs[i], "glyphs", i)?;
            if b.min.x < 0 || b.min.y < 0 {
                return Err(format!("glyphs[{i}] starts outside the texture"));
            }
            let k = &f.kerning[i];
            glyphs.push(FontGlyph { bounds: b.as_urect(), cropping: rect(&f.cropping[i], "cropping", i)?, left: k.x, width: k.y, right: k.z });
        }
        let default_character = match f.default_character.as_deref() {
            None | Some("") => None,
            Some(s) => {
                let mut chars = s.chars();
                match (chars.next(), chars.next()) {
                    (Some(c), None) => Some(c),
                    _ => return Err(format!("defaultCharacter is {s:?}, not one character")),
                }
            }
        };
        let index = characters.iter().enumerate().map(|(i, c)| (*c, i)).collect();
        Ok(SpriteFont { texture, glyphs, characters, index, line_spacing: f.vertical_line_spacing, spacing: f.horizontal_spacing, default_character })
    }
}

#[derive(Deserialize)]
struct JsonTexture {
    export: String,
}

#[derive(Deserialize)]
struct JsonRect {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

#[derive(Deserialize)]
struct JsonVec3 {
    x: f32,
    y: f32,
    z: f32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Json {
    glyphs: Vec<JsonRect>,
    cropping: Vec<JsonRect>,
    character_map: Vec<String>,
    vertical_line_spacing: f32,
    horizontal_spacing: f32,
    kerning: Vec<JsonVec3>,
    #[serde(default)]
    default_character: Option<String>,
}

#[derive(Deserialize)]
struct TextureOnly {
    texture: JsonTexture,
}

/// Loads a [`SpriteFont`] from its JSON, for a load typed `Handle<SpriteFont>`: Bevy picks a loader
/// by the asset type before the extension, so other JSON loads are untouched. The texture is
/// `texture.export`, beside the JSON in the same source, loaded with a nearest sampler.
#[derive(Default, TypePath)]
pub struct SpriteFontLoader;

impl AssetLoader for SpriteFontLoader {
    type Asset = SpriteFont;
    type Settings = ();
    type Error = String;

    async fn load(&self, reader: &mut dyn Reader, _: &(), cx: &mut LoadContext<'_>) -> Result<SpriteFont, String> {
        let at = cx.path().to_string();
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await.map_err(|e| format!("SpriteFont {at}: {e}"))?;
        let t: TextureOnly = serde_json::from_slice(&bytes).map_err(|e| format!("SpriteFont {at}: {e}"))?;
        let path = cx.path().resolve_embed_str(&t.texture.export).map_err(|e| format!("SpriteFont {at}: texture.export {:?}: {e}", t.texture.export))?;
        // The texture must be there: a font without it would draw nothing.
        cx.read_asset_bytes(path.clone()).await.map_err(|e| format!("SpriteFont {at}: its texture {path}: {e}"))?;
        let texture = cx.load_builder().with_settings(|s: &mut ImageLoaderSettings| s.sampler = ImageSampler::nearest()).load(path);
        SpriteFont::from_json(&bytes, texture).map_err(|e| format!("SpriteFont {at}: {e}"))
    }

    fn extensions(&self) -> &[&str] {
        &["json"]
    }
}
