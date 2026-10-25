//! GPU workarounds Bevy's renderer needs on phones, chosen before it makes its device.
//!
//! Skinned meshes break on Adreno (bevy#24926): on an Adreno GPU, no storage buffers per shader
//! stage, which moves skinning and morphs to uniform buffers and turns off GPU preprocessing and
//! storage-buffer clustering. Bevy picks its adapter inside its own setup, so the embedding asks
//! wgpu for the adapter first and sets the limits from what it found.

use bevy::render::settings::WgpuLimits;

/// The limits to hold Bevy's device to on the GPU named `name`, given its own: `None` off Adreno.
pub fn constrained_limits(name: &str, limits: &WgpuLimits) -> Option<WgpuLimits> {
    name.starts_with("Adreno").then(|| WgpuLimits { max_storage_buffers_per_shader_stage: 0, ..limits.clone() })
}

/// The first Android GPU's name and limits, from a throwaway wgpu instance.
#[cfg(target_os = "android")]
pub fn probe() -> Option<(String, WgpuLimits)> {
    let backends = wgpu::Backends::VULKAN | wgpu::Backends::GL;
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
    desc.backends = backends;
    let instance = wgpu::Instance::new(desc);
    let adapters = bevy::tasks::block_on(instance.enumerate_adapters(backends));
    adapters.first().map(|a| (a.get_info().name, a.limits()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_adreno_loses_its_storage_buffers() {
        let limits = WgpuLimits { max_storage_buffers_per_shader_stage: 8, max_texture_dimension_2d: 4096, ..WgpuLimits::default() };
        let held = constrained_limits("Adreno (TM) 740", &limits).unwrap();
        assert_eq!(held.max_storage_buffers_per_shader_stage, 0);
        assert_eq!(held.max_texture_dimension_2d, 4096, "every other limit is the GPU's own");
        assert!(constrained_limits("Mali-G715", &limits).is_none());
        assert!(constrained_limits("Apple M2", &limits).is_none());
    }
}
