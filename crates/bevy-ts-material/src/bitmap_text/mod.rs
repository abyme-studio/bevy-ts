//! Bitmap text (openspec change `pixel-art-2d`, stage 5): an XNA `SpriteFont`, as exported to JSON
//! beside its PNG, loads as a [`SpriteFont`] asset, and [`BitmapText`] lays a string out in it as
//! MonoGame does (`SpriteFont.cs` `MeasureString`, `SpriteBatch.cs` `DrawString`). Each glyph is a
//! child entity with a `Sprite` cut from the font's texture by `rect`, so Bevy batches a label into
//! one draw. The glyph children are this module's, marked [`Glyph`]: they have no TSX node, so the
//! reconciler keeps them and levels never write them; they are laid out again when the text, font,
//! colour, scale or `Anchor` changes, and go with `BitmapText`.

pub mod font;

pub use font::{FontGlyph, SpriteFont, SpriteFontLoader};

use bevy::prelude::*;
use bevy::sprite::Anchor;
use bevy_ts::log::{LogLevel, ScriptLog};
use std::collections::HashSet;

/// A label in a bitmap font, laid out from the entity's origin in pixels (y up), its measured box
/// placed by the entity's `Anchor` as `Text2d`'s is, its point of the box rounded down to a whole
/// pixel. `scale` multiplies every position and size, so positions stay whole pixels times `scale`.
#[derive(Component, Reflect, Clone, Debug, PartialEq)]
#[reflect(Component, Default)]
#[require(Transform, Visibility, Anchor, BitmapTextLaid)]
pub struct BitmapText {
    pub text: String,
    pub font: Handle<SpriteFont>,
    /// Tints every glyph.
    pub color: Color,
    pub scale: f32,
}

impl Default for BitmapText {
    fn default() -> Self {
        Self { text: String::new(), font: Handle::default(), color: Color::WHITE, scale: 1.0 }
    }
}

/// A glyph sprite of a [`BitmapText`], its child. Written by bitmap text only.
#[derive(Component, Clone, Copy, Debug)]
pub struct Glyph;

/// What the last layout made: the measured box, in the entity's space, and the glyphs drawn.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct BitmapTextLaid {
    pub size: Vec2,
    pub glyphs: usize,
}

/// One glyph as MonoGame places it: its index in the font, and its top-left corner in pixels from
/// the label's top-left, y down.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub glyph: usize,
    pub at: Vec2,
}

/// A string laid out: its glyphs, its measured size (MonoGame's `MeasureString`), and the
/// characters the font has no glyph for and no default to draw instead.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    pub glyphs: Vec<Placed>,
    pub size: Vec2,
    pub missing: Vec<char>,
}

/// MonoGame's layout of `text` in `font`: at a line's first glyph x is its left bearing if
/// positive, else 0; after it x grows by the spacing plus the next glyph's left bearing; a glyph
/// draws at x plus its cropping offset, then x grows by its width and right bearing; `\n` starts a
/// line `line_spacing` lower; `\r` is skipped. A character outside the map draws the default
/// character, or is skipped when there is none.
pub fn layout(font: &SpriteFont, text: &str) -> Layout {
    let mut out = Layout::default();
    if text.is_empty() {
        return out;
    }
    let (mut x, mut y, mut width) = (0.0f32, 0.0f32, 0.0f32);
    let mut line_height = font.line_spacing;
    let mut first = true;
    for c in text.chars() {
        match c {
            '\r' => continue,
            '\n' => {
                line_height = font.line_spacing;
                x = 0.0;
                y += font.line_spacing;
                first = true;
                continue;
            }
            _ => {}
        }
        let Some(i) = font.glyph_index(c) else {
            out.missing.push(c);
            continue;
        };
        let g = &font.glyphs[i];
        if first {
            x = g.left.max(0.0);
            first = false;
        } else {
            x += font.spacing + g.left;
        }
        out.glyphs.push(Placed { glyph: i, at: Vec2::new(x + g.cropping.min.x as f32, y + g.cropping.min.y as f32) });
        x += g.width;
        width = width.max(x + g.right.max(0.0));
        x += g.right;
        line_height = line_height.max(g.cropping.height() as f32);
    }
    out.size = Vec2::new(width, y + line_height);
    out
}

/// Bitmap fonts and labels. The engine adds it; a Rust app adds it beside `DefaultPlugins`.
pub struct BitmapTextPlugin;

impl Plugin for BitmapTextPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<SpriteFont>()
            .register_asset_loader(SpriteFontLoader)
            .register_type::<BitmapText>()
            .add_systems(PostUpdate, lay_out.in_set(BitmapTextSystems).before(bevy::transform::TransformSystems::Propagate))
            .add_observer(|remove: On<Remove<BitmapText>>, children: Query<&Children>, glyphs: Query<(), With<Glyph>>, mut commands: Commands| {
                for c in children.iter_descendants(remove.entity).filter(|c| glyphs.contains(*c)) {
                    commands.entity(c).try_despawn();
                }
            });
    }
}

/// The system that lays out changed labels, in `PostUpdate` before transform propagation: after
/// the TSX reconcile, so a bound label draws the frame its binding changes.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BitmapTextSystems;

/// Labels to lay out: changed, or waiting on their font.
#[derive(Default)]
struct Pending(HashSet<Entity>);

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn lay_out(
    mut commands: Commands,
    changed: Query<Entity, Or<(Changed<BitmapText>, Changed<Anchor>)>>,
    labels: Query<(Entity, &BitmapText, &Anchor, Option<&Children>, &BitmapTextLaid)>,
    glyphs: Query<(), With<Glyph>>,
    fonts: Res<Assets<SpriteFont>>,
    mut events: MessageReader<AssetEvent<SpriteFont>>,
    mut pending: Local<Pending>,
    mut told: Local<HashSet<char>>,
    mut script_log: Option<ResMut<ScriptLog>>,
    generations: Option<Res<bevy_ts::generation::Generations>>,
) {
    pending.0.extend(changed.iter());
    // A font that loads or reloads lays out every label in it again.
    let reloaded: HashSet<AssetId<SpriteFont>> = events
        .read()
        .filter_map(|e| match e {
            AssetEvent::LoadedWithDependencies { id } | AssetEvent::Modified { id } => Some(*id),
            _ => None,
        })
        .collect();
    if !reloaded.is_empty() {
        pending.0.extend(labels.iter().filter(|(_, l, ..)| reloaded.contains(&l.font.id())).map(|(e, ..)| e));
    }
    let todo: Vec<Entity> = pending.0.drain().collect();
    for e in todo {
        let Ok((_, label, anchor, children, laid)) = labels.get(e) else { continue };
        let Some(font) = fonts.get(&label.font) else {
            // Not loaded yet: waits for it.
            pending.0.insert(e);
            continue;
        };
        let l = layout(font, &label.text);
        for c in l.missing.iter().filter(|c| told.insert(**c)) {
            let message = format!("entity {e}: bitmap text has no glyph for {c:?} and its font has no defaultCharacter: skipped");
            match &mut script_log {
                Some(log) => log.push(LogLevel::Warn, message, None, generations.as_ref().map_or(0, |g| g.live)),
                None => warn!("{message}"),
            }
        }
        for c in children.into_iter().flatten().filter(|c| glyphs.contains(**c)) {
            commands.entity(*c).despawn();
        }
        let s = label.scale;
        // The anchor's point of the box, in whole font pixels from its top-left (y down, rounded
        // down so an odd-sized box keeps glyphs on whole pixels), goes to the origin.
        let point = Vec2::new((anchor.0.x + 0.5) * l.size.x, (0.5 - anchor.0.y) * l.size.y).floor();
        let shift = Vec2::new(-point.x, point.y) * s;
        for p in &l.glyphs {
            let g = &font.glyphs[p.glyph];
            let size = g.bounds.size().as_vec2();
            let centre = Vec2::new(p.at.x + size.x / 2.0, -(p.at.y + size.y / 2.0)) * s + shift;
            let sprite = Sprite { image: font.texture.clone(), rect: Some(g.bounds.as_rect()), custom_size: Some(size * s), color: label.color, ..default() };
            commands.spawn((Glyph, sprite, Transform::from_translation(centre.extend(0.0)), ChildOf(e)));
        }
        let now = BitmapTextLaid { size: l.size * s, glyphs: l.glyphs.len() };
        if *laid != now {
            commands.entity(e).insert(now);
        }
    }
}
