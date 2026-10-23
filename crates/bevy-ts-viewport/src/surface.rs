//! The ring: three IOSurfaces wrapped as Metal textures on wgpu's own device, then as
//! `wgpu::Texture`s Bevy renders into. The only module that touches wgpu's hal.

use bevy::render::render_resource::{Texture, TextureView};
use bevy::render::renderer::RenderDevice;
use bevy::render::render_resource::TextureFormat;
use objc2_core_foundation::{CFDictionary, CFNumber, CFRetained, CFString, CFType};
use objc2_io_surface::{IOSurfaceRef, kIOSurfaceBytesPerElement, kIOSurfaceColorSpace, kIOSurfaceHeight, kIOSurfacePixelFormat, kIOSurfaceWidth};
use objc2_metal::{MTLDevice, MTLPixelFormat, MTLStorageMode, MTLTextureDescriptor, MTLTextureType, MTLTextureUsage};

/// Slots in a ring: one being written, one shown, one the window server may still hold.
pub const SLOTS: usize = 3;
/// Ring sizes are rounded up to this, so a live resize makes few rings.
pub const ROUND: u32 = 256;
/// The texture format every slot is viewed as. Its IOSurface is `BGRA`, tagged sRGB.
pub const FORMAT: TextureFormat = TextureFormat::Bgra8UnormSrgb;

/// An IOSurface. IOSurfaces are safe to use from any thread.
#[derive(Clone)]
pub struct Surface(pub CFRetained<IOSurfaceRef>);
unsafe impl Send for Surface {}
unsafe impl Sync for Surface {}

impl Surface {
    /// A `BGRA` surface of `w` by `h` pixels, tagged sRGB.
    pub fn new(w: u32, h: u32) -> Option<Surface> {
        let n = |v: i32| CFNumber::new_i32(v);
        let (w_, h_, bpe, fmt) = (n(w as i32), n(h as i32), n(4), n(i32::from_be_bytes(*b"BGRA")));
        let keys: [&CFString; 4] = unsafe { [kIOSurfaceWidth, kIOSurfaceHeight, kIOSurfaceBytesPerElement, kIOSurfacePixelFormat] };
        let values: [&CFType; 4] = [&w_, &h_, &bpe, &fmt];
        let props = CFDictionary::<CFString, CFType>::from_slices(&keys, &values);
        let s = unsafe { IOSurfaceRef::new(props.as_opaque()) }?;
        let srgb = CFString::from_static_str("kCGColorSpaceSRGB");
        unsafe { s.set_value(kIOSurfaceColorSpace, &srgb) };
        Some(Surface(s))
    }

    pub fn width(&self) -> u32 {
        self.0.width() as u32
    }

    pub fn height(&self) -> u32 {
        self.0.height() as u32
    }

    /// Whether any process (the window server compositing it, a viewer) is using it.
    pub fn in_use(&self) -> bool {
        self.0.is_in_use()
    }

    /// A Mach send right for the surface, owned by the caller.
    pub fn mach_port(&self) -> u32 {
        self.0.create_mach_port()
    }

    /// The surface a received Mach port names.
    pub fn from_mach_port(port: u32) -> Option<Surface> {
        IOSurfaceRef::lookup_from_mach_port(port).map(Surface)
    }

    /// Copies the top-left `w` by `h` pixels, `BGRA` rows with no padding.
    pub fn read_rect(&self, w: u32, h: u32) -> Vec<u8> {
        use objc2_io_surface::IOSurfaceLockOptions;
        let s = &self.0;
        let (w, h) = (w.min(self.width()) as usize, h.min(self.height()) as usize);
        let mut seed = 0;
        unsafe { s.lock(IOSurfaceLockOptions::ReadOnly, &mut seed) };
        let base = s.base_address().as_ptr() as *const u8;
        let row = s.bytes_per_row();
        let mut out = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            out.extend_from_slice(unsafe { std::slice::from_raw_parts(base.add(y * row), w * 4) });
        }
        unsafe { s.unlock(IOSurfaceLockOptions::ReadOnly, &mut seed) };
        out
    }

    /// Reads pixel (`x`, `y`) as `[b, g, r, a]`, for tests and the throwaway viewer.
    pub fn read(&self, points: &[(u32, u32)]) -> Vec<[u8; 4]> {
        use objc2_io_surface::IOSurfaceLockOptions;
        let s = &self.0;
        let mut seed = 0;
        unsafe { s.lock(IOSurfaceLockOptions::ReadOnly, &mut seed) };
        let base = s.base_address().as_ptr() as *const u8;
        let row = s.bytes_per_row();
        let out = points
            .iter()
            .map(|&(x, y)| {
                let p = unsafe { base.add(y as usize * row + x as usize * 4) };
                let mut px = [0u8; 4];
                unsafe { std::ptr::copy_nonoverlapping(p, px.as_mut_ptr(), 4) };
                px
            })
            .collect();
        unsafe { s.unlock(IOSurfaceLockOptions::ReadOnly, &mut seed) };
        out
    }
}

/// One slot: the surface and the wgpu texture over the same memory.
pub struct Slot {
    pub surface: Surface,
    pub texture: Texture,
    pub view: TextureView,
}

/// Rounds a pixel size up to the ring allocation.
pub fn round_up(w: u32, h: u32) -> (u32, u32) {
    (w.max(1).div_ceil(ROUND) * ROUND, h.max(1).div_ceil(ROUND) * ROUND)
}

/// Wraps the top-left `w` by `h` pixels of `surface` as a texture on Bevy's device; `None` if
/// the device is not Metal or the size does not fit.
pub fn make_slot(device: &RenderDevice, surface: &Surface, w: u32, h: u32) -> Option<Slot> {
    if w > surface.width() || h > surface.height() {
        return None;
    }
    let surface = surface.clone();
    let device = device.wgpu_device();
    let hal = unsafe { device.as_hal::<wgpu_hal::api::Metal>() }?;
    let desc = unsafe {
        MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(MTLPixelFormat::BGRA8Unorm_sRGB, w as usize, h as usize, false)
    };
    desc.setUsage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
    desc.setStorageMode(MTLStorageMode::Shared);
    let raw = hal.raw_device().newTextureWithDescriptor_iosurface_plane(&desc, &surface.0, 0)?;
    drop(hal);
    // wgpu holds the surface as long as it holds the texture.
    let keep = surface.clone();
    let hal_texture = unsafe {
        wgpu_hal::metal::Device::texture_from_raw(
            raw,
            FORMAT,
            MTLTextureType::Type2D,
            1,
            1,
            wgpu_hal::CopyExtent { width: w, height: h, depth: 1 },
            Some(Box::new(move || drop(keep))),
        )
    };
    let texture = unsafe {
        device.create_texture_from_hal::<wgpu_hal::api::Metal>(
            hal_texture,
            &wgpu::TextureDescriptor {
                label: Some("bevy-ts viewport slot"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            },
            wgpu::TextureUses::UNINITIALIZED,
        )
    };
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    Some(Slot { surface, texture: Texture::from(texture), view: TextureView::from(view) })
}
