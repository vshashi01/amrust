use crate::prelude::*;

use crate::{
    clip::{self, ClipPlanes},
    texture,
    transparency::{self, Transparency},
};

use std::num::NonZero;

pub struct RenderDataLocalResources {
    pub transparency: Transparency,
    pub clip_plane: ClipPlanes<1>,
    pub(crate) uniform_bg: wgpu::BindGroup,
    pub(crate) texture_bg: Option<wgpu::BindGroup>,
}

impl RenderDataLocalResources {
    pub fn clip_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        // clip::create_clip_bind_group_layout::<1, 0>(device, include_transparency)
        create_general_frag_uniform_bgl(device)
    }

    pub fn textured_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        texture::creae_single_texture_bgl::<0, 1>(
            device,
            "Mesh Local Textured Bind Group Layout",
            true,
        )
    }

    pub fn array_textured_layout(
        device: &wgpu::Device,
        count: NonZero<u32>,
    ) -> wgpu::BindGroupLayout {
        texture::create_array_texture_bgl::<0, 1>(
            device,
            "Mesh Local Array Texture BGL",
            true,
            count,
        )
    }

    pub fn new_colored(
        device: &wgpu::Device,
        clip_plane: ClipPlanes<1>,
        transparency: Transparency,
    ) -> Self {
        let clip_layout = Self::clip_layout(device);
        // let clip_bind_group =
        //     clip::create_clip_bind_group::<1, 0>(device, &clip_layout, &clip_plane.buffers);
        let uniform_bg =
            create_general_frag_uniform_bg(device, &clip_layout, &clip_plane, &transparency);

        Self {
            clip_plane,
            uniform_bg,
            texture_bg: None,
            transparency,
        }
    }

    pub fn update_to_gpu(&self, queue: &wgpu::Queue) {
        self.transparency.write_buffer(queue);
        self.clip_plane.write_buffer(queue);
    }

    pub fn new_textured(
        device: &wgpu::Device,
        clip_plane: ClipPlanes<1>,
        texture_view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
        transparency: Transparency,
    ) -> Self {
        let clip_layout = Self::clip_layout(device);
        // let clip_bind_group =
        //     clip::create_clip_bind_group::<1, 0>(device, &clip_layout, &clip_plane.buffers);
        let uniform_bg =
            create_general_frag_uniform_bg(device, &clip_layout, &clip_plane, &transparency);

        let res_layout = Self::textured_layout(device);
        let resource_bind_group = texture::create_single_texture_bg::<0, 1>(
            device,
            "",
            texture_view,
            sampler,
            &res_layout,
        );

        Self {
            clip_plane,
            uniform_bg,
            texture_bg: Some(resource_bind_group),
            transparency,
        }
    }

    pub fn new_array_textured<const TEXTURE_COUNT: u32>(
        device: &wgpu::Device,
        clip_plane: ClipPlanes<1>,
        texture_views: &[&wgpu::TextureView],
        sampler: &wgpu::Sampler,
        transparency: Transparency,
    ) -> Self {
        if TEXTURE_COUNT > texture::MAX_BINDING_ARRAY_ELEMENTS_PER_SHADER_STAGE {
            panic!(
                "Too many textures in texture array: Max textures are {}",
                texture::MAX_BINDING_ARRAY_ELEMENTS_PER_SHADER_STAGE
            );
        } else if TEXTURE_COUNT == 0 {
            panic!("Zero texture in texture array is not supported")
        }

        let clip_layout = Self::clip_layout(device);
        // let clip_bind_group =
        //     clip::create_clip_bind_group::<1, 0>(device, &clip_layout, &clip_plane.buffers);
        let uniform_bg =
            create_general_frag_uniform_bg(device, &clip_layout, &clip_plane, &transparency);

        let res_layout = Self::array_textured_layout(device, NonZero::new(TEXTURE_COUNT).unwrap());
        let resource_bind_group = texture::create_array_texture_bg::<0, 1>(
            device,
            "Mesh Local Array Textured Bind Group",
            texture_views,
            sampler,
            &res_layout,
        );
        Self {
            clip_plane,
            uniform_bg,
            texture_bg: Some(resource_bind_group),
            transparency,
        }
    }
}

fn create_general_frag_uniform_bgl(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let mut entries: Vec<wgpu::BindGroupLayoutEntry> = Vec::new();

    entries.push(transparency::create_transparency_bind_group_layout_entry(0));

    let mut clip_entries = clip::create_clip_bind_group_layout_entries::<1, 1>();
    entries.append(&mut clip_entries);

    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Mesh Local Fragment Shader Uniform"),
        entries: &entries,
    })
}

fn create_general_frag_uniform_bg<'a>(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    clip_planes: &'a ClipPlanes<1>,
    transparency: &'a Transparency,
) -> wgpu::BindGroup {
    let mut entries: Vec<wgpu::BindGroupEntry> = Vec::new();

    entries.push(transparency::create_transparency_bind_group_entry(
        0,
        transparency.get_binding_resource(),
    ));

    let mut clip_entries = clip::create_clip_bind_group_entries::<1, 1>(&clip_planes.buffers);
    entries.append(&mut clip_entries);

    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Mesh Local Fragment Uniform"),
        layout,
        entries: &entries,
    })
}
