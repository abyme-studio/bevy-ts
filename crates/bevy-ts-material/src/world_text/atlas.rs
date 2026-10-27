//! The one SDF atlas all world text shares (design 6): 4096 texels wide, 256 high at first (1 MB),
//! its height doubled as glyphs fill it, up to 4096 (16 MB). Cells are placed by a shelf packer and
//! keyed by the Bevy glyph raster they were made from, so each glyph Bevy rasterises becomes a
//! distance field once. Mesh UVs are in texels, so growing rebuilds no mesh. A frame's new glyphs
//! are placed together, sorted by their raster, so the atlas does not depend on the order labels
//! were laid out in that frame.

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use super::sdf::{SPREAD, sdf};

pub const WIDTH: u32 = 4096;
pub const START_HEIGHT: u32 = 256;
/// 4096 × 4096 R8 texels: 16 MB.
pub const MAX_HEIGHT: u32 = 4096;
/// Texels left empty right of and below each cell, so filtering never reads a neighbour.
const GAP: u32 = 1;

/// A shelf packer: rows of cells, each row as tall as the first cell that opened it.
#[derive(Clone, Debug)]
pub struct Packer {
    pub width: u32,
    pub height: u32,
    pub max_height: u32,
    shelves: Vec<Shelf>,
    top: u32,
}

#[derive(Clone, Copy, Debug)]
struct Shelf {
    y: u32,
    h: u32,
    x: u32,
}

impl Packer {
    pub fn new(width: u32, height: u32, max_height: u32) -> Self {
        Self { width, height, max_height, shelves: Vec::new(), top: 0 }
    }

    /// The top-left corner of a new `w` × `h` cell, the height doubled first when the cell needs it;
    /// `None` when it cannot fit under `max_height`.
    pub fn place(&mut self, w: u32, h: u32) -> Option<UVec2> {
        let (w, h) = (w + GAP, h + GAP);
        if w > self.width {
            return None;
        }
        let width = self.width;
        if let Some(s) = self.shelves.iter_mut().filter(|s| s.h >= h && s.x + w <= width).min_by_key(|s| s.h) {
            let at = UVec2::new(s.x, s.y);
            s.x += w;
            return Some(at);
        }
        if self.top + h > self.max_height {
            return None;
        }
        while self.top + h > self.height {
            self.height = (self.height * 2).min(self.max_height);
        }
        self.shelves.push(Shelf { y: self.top, h, x: w });
        let at = UVec2::new(0, self.top);
        self.top += h;
        Some(at)
    }
}

/// A glyph's distance field in the atlas: its top-left texel and its size, the spread included.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cell {
    pub min: UVec2,
    pub size: UVec2,
}

/// A glyph raster of Bevy's font atlas: its texture and its rect there.
type Key = (AssetId<Image>, URect);

/// The shared atlas: its image (R8, kept on the CPU to add to), its packer, and every glyph made.
#[derive(Resource)]
pub struct WorldTextAtlas {
    pub image: Handle<Image>,
    pub packer: Packer,
    cells: HashMap<Key, Option<Cell>>,
    /// Glyphs asked for since the last `place`, with their coverage.
    pending: HashMap<Key, Vec<u8>>,
    /// Every raster made, by its size and coverage: two glyphs of the same pixels share a cell.
    rasters: HashMap<(UVec2, Vec<u8>), Option<Cell>>,
    /// Glyph rasters turned into distance fields.
    pub made: usize,
    /// Glyphs refused because the atlas was full; each logged one error.
    pub refused: usize,
    /// Written since the materials last saw it.
    pub(crate) dirty: bool,
}

impl WorldTextAtlas {
    /// An empty atlas `height` high that grows to `max_height` (the engine's: 256 and 4096).
    pub fn new(images: &mut Assets<Image>, height: u32, max_height: u32) -> Self {
        let mut image = Image::new_fill(
            Extent3d { width: WIDTH, height, depth_or_array_layers: 1 },
            TextureDimension::D2,
            &[0],
            TextureFormat::R8Unorm,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::ClampToEdge,
            address_mode_v: ImageAddressMode::ClampToEdge,
            mag_filter: ImageFilterMode::Linear,
            min_filter: ImageFilterMode::Linear,
            ..default()
        });
        Self { image: images.add(image), packer: Packer::new(WIDTH, height, max_height), cells: HashMap::default(), pending: HashMap::default(), rasters: HashMap::default(), made: 0, refused: 0, dirty: false }
    }

    /// Asks for the glyph raster at `rect` in Bevy's atlas `texture`; `place` makes it. False when
    /// Bevy's raster has no CPU copy: the glyph is not kept, and the next layout of its label tries
    /// again.
    pub fn want(&mut self, images: &Assets<Image>, texture: AssetId<Image>, rect: URect) -> bool {
        let key = (texture, rect);
        if self.cells.contains_key(&key) || self.pending.contains_key(&key) {
            return true;
        }
        // Bevy's glyph atlas is RGBA8, the coverage in alpha.
        let Some(alpha) = raster(images, texture, rect) else {
            warn!("world text: Bevy's raster of a glyph ({texture:?}, {rect:?}) has no CPU data; it draws nothing this time");
            return false;
        };
        self.pending.insert(key, alpha);
        true
    }

    /// Makes every glyph asked for since the last call, in the order of their rasters (tallest
    /// first, so shorter ones fill its shelf; then width, then coverage), not the order they were
    /// asked for.
    pub fn place(&mut self, images: &mut Assets<Image>) {
        let mut pending: Vec<_> = self.pending.drain().map(|((t, r), a)| (r.size(), a, t, r)).collect();
        pending.sort_unstable_by(|a, b| (b.0.y, b.0.x, &a.1).cmp(&(a.0.y, a.0.x, &b.1)));
        for (size, alpha, texture, rect) in pending {
            let raster = (size, alpha);
            let made = match self.rasters.get(&raster) {
                Some(c) => *c,
                None => {
                    let c = self.make(images, &raster.1, rect);
                    self.rasters.insert(raster, c);
                    c
                }
            };
            self.cells.insert((texture, rect), made);
        }
    }

    /// The cell of the glyph raster at `rect` in Bevy's atlas `texture`, once placed; `None` for one
    /// not placed or refused because the atlas is full.
    pub fn cell(&self, texture: AssetId<Image>, rect: URect) -> Option<Cell> {
        self.cells.get(&(texture, rect)).copied().flatten()
    }

    fn make(&mut self, images: &mut Assets<Image>, alpha: &[u8], rect: URect) -> Option<Cell> {
        let (w, h) = (rect.width() as usize, rect.height() as usize);
        let s = SPREAD as usize;
        let size = UVec2::new((w + 2 * s) as u32, (h + 2 * s) as u32);
        let Some(min) = self.packer.place(size.x, size.y) else {
            self.refused += 1;
            error!(
                "world text: the glyph atlas is full ({WIDTH} × {} texels, {} MB); this glyph draws nothing, the rest of its label draws",
                self.packer.max_height,
                (WIDTH as u64 * self.packer.max_height as u64) >> 20
            );
            return None;
        };
        let field = sdf(alpha, w, h, s);
        let mut image = images.get_mut(&self.image)?;
        if image.height() < self.packer.height {
            image.resize(Extent3d { width: WIDTH, height: self.packer.height, depth_or_array_layers: 1 });
        }
        let data = image.data.as_mut()?;
        for y in 0..size.y as usize {
            let at = (min.y as usize + y) * WIDTH as usize + min.x as usize;
            data[at..at + size.x as usize].copy_from_slice(&field[y * size.x as usize..(y + 1) * size.x as usize]);
        }
        self.made += 1;
        self.dirty = true;
        Some(Cell { min, size })
    }
}

/// The coverage of the glyph raster at `rect` in Bevy's RGBA8 atlas `texture`, from its alpha.
fn raster(images: &Assets<Image>, texture: AssetId<Image>, rect: URect) -> Option<Vec<u8>> {
    let src = images.get(texture)?;
    let (sw, data) = (src.width() as usize, src.data.as_ref()?);
    let (w, h) = (rect.width() as usize, rect.height() as usize);
    Some((0..h).flat_map(|y| (0..w).map(move |x| ((rect.min.y as usize + y) * sw + rect.min.x as usize + x) * 4 + 3)).map(|i| data[i]).collect())
}
