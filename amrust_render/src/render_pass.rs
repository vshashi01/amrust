use crate::{RenderData, Renderable, material, screen_space, transformation, vertex};
use crate::{constants, prelude::*};

use std::collections::HashMap;

pub fn surface_3d_render_pass_with_depth<'a>(
    renderables: &[RenderData<'a>],
    render_pipeline_cache: &HashMap<&'static str, wgpu::RenderPipeline>,
    render_pass: &mut wgpu::RenderPass<'_>,
) {
    let single_textured_objects = renderables.iter().filter_map(|o| match o.renderable {
        Renderable::TexturedMesh => Some((
            o.mesh,
            o.instance,
            o.local_bind_groups.clone(),
            constants::TEXTURE_MESH_PIPELINE_KEY,
        )),
        Renderable::ArrayTexturedMesh => Some((
            o.mesh,
            o.instance,
            o.local_bind_groups.clone(),
            constants::ARRAY_TEXTURE_MESH_PIPELINE_KEY,
        )),
        _ => None,
    });

    for (mesh, instance, bind_groups_list, pipeline_key) in single_textured_objects {
        render_pass.set_pipeline(render_pipeline_cache.get(pipeline_key).unwrap());
        let position_buffer = mesh.vertex_slice::<vertex::Position>();
        let color_buffer = mesh.vertex_slice::<vertex::Color>();
        let tex_coord_buffer = mesh.vertex_slice::<vertex::TexCoords>();
        let use_texture_buffer = mesh.vertex_slice::<vertex::UseTexture>();

        let transformation_buffer = instance.vertex_slice::<transformation::TransformationData>();
        let material_buffer = instance.vertex_slice::<material::RgbMaterialData>();

        render_pass.set_vertex_buffer(0, position_buffer);
        render_pass.set_vertex_buffer(1, color_buffer);
        render_pass.set_vertex_buffer(2, tex_coord_buffer);
        render_pass.set_vertex_buffer(3, use_texture_buffer);
        render_pass.set_vertex_buffer(4, transformation_buffer);
        render_pass.set_vertex_buffer(5, material_buffer);

        for pair in bind_groups_list.iter() {
            render_pass.set_bind_group(pair.1, pair.0, &[]);
        }

        if let Some(index_stream) = &mesh.mesh_index_stream {
            let index_buffer = mesh.buffer.slice(index_stream.offset..index_stream.end);
            render_pass.set_index_buffer(index_buffer, index_stream.format);

            render_pass.draw_indexed(0..index_stream.index_count, 0, 0..instance.instance_count);
        } else {
            panic!("Mesh does not have an index buffer");
        }
    }

    let colored_objects = renderables.iter().filter_map(|o| {
        if let Renderable::ColoredMesh = &o.renderable {
            Some((o.mesh, o.instance))
        } else {
            None
        }
    });

    for (mesh, instance) in colored_objects {
        render_pass.set_pipeline(
            render_pipeline_cache
                .get(constants::VERTEX_COLORED_MESH_PIPELINE_KEY)
                .unwrap(),
        );
        let position_buffer = mesh.vertex_slice::<vertex::Position>();
        let color_buffer = mesh.vertex_slice::<vertex::Color>();

        let transformation_buffer = instance.vertex_slice::<transformation::TransformationData>();
        let material_buffer = instance.vertex_slice::<material::RgbMaterialData>();

        render_pass.set_vertex_buffer(0, position_buffer);
        render_pass.set_vertex_buffer(1, color_buffer);
        render_pass.set_vertex_buffer(2, transformation_buffer);
        render_pass.set_vertex_buffer(3, material_buffer);

        if let Some(index_stream) = &mesh.mesh_index_stream {
            let index_buffer = mesh.buffer.slice(index_stream.offset..index_stream.end);
            render_pass.set_index_buffer(index_buffer, index_stream.format);

            render_pass.draw_indexed(0..index_stream.index_count, 0, 0..instance.instance_count);
        } else {
            panic!("Mesh does not have an index buffer");
        }
    }

    let simple_objects = renderables.iter().filter_map(|o| {
        if let Renderable::Mesh = &o.renderable {
            Some((o.mesh, o.instance))
        } else {
            None
        }
    });

    for (mesh, instance) in simple_objects {
        render_pass.set_pipeline(
            render_pipeline_cache
                .get(constants::SOLID_COLORED_MESH_PIPELINE_KEY)
                .unwrap(),
        );
        let position_buffer = mesh.vertex_slice::<vertex::Position>();
        let transformation_buffer = instance.vertex_slice::<transformation::TransformationData>();
        let material_buffer = instance.vertex_slice::<material::RgbMaterialData>();

        render_pass.set_vertex_buffer(0, position_buffer);
        render_pass.set_vertex_buffer(1, transformation_buffer);
        render_pass.set_vertex_buffer(2, material_buffer);

        render_pass.draw(0..mesh.vertex_count, 0..instance.instance_count);
    }

    screen_space_colored_mesh_pass(renderables, render_pipeline_cache, render_pass, true);
}

pub fn wireframe_3d_render_pass<'a>(
    renderables: &[RenderData<'a>],
    render_pipeline_cache: &HashMap<&'static str, wgpu::RenderPipeline>,
    render_pass: &mut wgpu::RenderPass<'_>,
) {
    let wireframe_objects = renderables.iter().filter_map(|o| {
        if let Renderable::WireframeMesh = &o.renderable {
            Some((o.mesh, o.instance))
        } else {
            None
        }
    });

    for (mesh, instance) in wireframe_objects {
        render_pass.set_pipeline(
            render_pipeline_cache
                .get(constants::WIREFRAME_MESH_PIPELINE_KEY)
                .unwrap(),
        );

        let position_buffer = mesh.vertex_slice::<vertex::Position>();
        let transformation_buffer = instance.vertex_slice::<transformation::TransformationData>();
        let material_buffer = instance.vertex_slice::<material::RgbMaterialData>();

        render_pass.set_vertex_buffer(0, position_buffer);
        render_pass.set_vertex_buffer(1, transformation_buffer);
        render_pass.set_vertex_buffer(2, material_buffer);

        if let Some(index_stream) = &mesh.wireframe_index_stream {
            let index_buffer = mesh.buffer.slice(index_stream.offset..index_stream.end);
            render_pass.set_index_buffer(index_buffer, index_stream.format);

            render_pass.draw_indexed(0..index_stream.index_count, 0, 0..instance.instance_count);
        } else {
            render_pass.draw(0..mesh.vertex_count, 0..instance.instance_count);
        }
    }

    screen_space_wireframe_pass(renderables, render_pipeline_cache, render_pass, true);
}

pub fn silhoutte_pass<'a>(
    renderables: &[RenderData<'a>],
    render_pipeline_cache: &HashMap<&'static str, wgpu::RenderPipeline>,
    render_pass: &mut wgpu::RenderPass<'_>,
) {
    let silhoutte_objs = renderables.iter().filter_map(|o| {
        if let Renderable::SilhouetteMesh = &o.renderable {
            Some((o.mesh, o.instance))
        } else {
            None
        }
    });

    render_pass.set_pipeline(
        render_pipeline_cache
            .get(constants::MESH_SILHOUETTE_PIPELINE_KEY)
            .unwrap(),
    );

    for (mesh, instance) in silhoutte_objs {
        let position_buffer = mesh.vertex_slice::<vertex::Position>();
        let transformation_buffer = instance.vertex_slice::<transformation::TransformationData>();

        render_pass.set_vertex_buffer(0, position_buffer);
        render_pass.set_vertex_buffer(1, transformation_buffer);

        if let Some(index_stream) = &mesh.mesh_index_stream {
            let index_buffer = mesh.buffer.slice(index_stream.offset..index_stream.end);
            render_pass.set_index_buffer(index_buffer, index_stream.format);

            render_pass.draw_indexed(0..index_stream.index_count, 0, 0..instance.instance_count);
        } else {
            render_pass.draw(0..mesh.vertex_count, 0..instance.instance_count);
        }
    }
}

pub fn screen_space_colored_mesh_pass<'a>(
    renderables: &[RenderData<'a>],
    render_pipeline_cache: &HashMap<&'static str, wgpu::RenderPipeline>,
    render_pass: &mut wgpu::RenderPass<'_>,
    is_depth_tested: bool,
) {
    let screen_space_objs = renderables.iter().filter_map(|o| {
        if let Renderable::ScreeSpaceColoredMesh { depth_testing, .. } = &o.renderable {
            if *depth_testing == is_depth_tested {
                Some((o.mesh, o.instance))
            } else {
                None
            }
        } else {
            None
        }
    });

    if is_depth_tested {
        render_pass.set_pipeline(
            render_pipeline_cache
                .get(constants::SCREEN_SPACE_MESH_PIPELINE_KEY)
                .unwrap(),
        );
    } else {
        render_pass.set_pipeline(
            render_pipeline_cache
                .get(constants::SCREEN_SPACE_MESH_WITHOUT_DEPTH_PIPELINE_KEY)
                .unwrap(),
        );
    }

    for (mesh, instance) in screen_space_objs {
        let position_buffer = mesh.vertex_slice::<vertex::Position>();
        let color_buffer = mesh.vertex_slice::<vertex::Color>();
        let transformation_buffer = instance.vertex_slice::<transformation::TransformationData>();
        let material_buffer = instance.vertex_slice::<material::RgbMaterialData>();
        let size_in_pixel_buffer = instance.vertex_slice::<screen_space::SizeInPixel>();

        render_pass.set_vertex_buffer(0, position_buffer);
        render_pass.set_vertex_buffer(1, color_buffer);
        render_pass.set_vertex_buffer(2, transformation_buffer);
        render_pass.set_vertex_buffer(3, material_buffer);
        render_pass.set_vertex_buffer(4, size_in_pixel_buffer);

        if let Some(index_stream) = &mesh.mesh_index_stream {
            let index_buffer = mesh.buffer.slice(index_stream.offset..index_stream.end);
            render_pass.set_index_buffer(index_buffer, index_stream.format);

            render_pass.draw_indexed(0..index_stream.index_count, 0, 0..instance.instance_count);
        } else {
            render_pass.draw(0..mesh.vertex_count, 0..instance.instance_count);
        }
    }
}

pub fn screen_space_wireframe_pass<'a>(
    renderables: &[RenderData<'a>],
    render_pipeline_cache: &HashMap<&'static str, wgpu::RenderPipeline>,
    render_pass: &mut wgpu::RenderPass<'_>,
    is_depth_tested: bool,
) {
    let screen_space_objs = renderables.iter().filter_map(|o| {
        if let Renderable::ScreenSpaceWireframeMesh { depth_testing, .. } = &o.renderable {
            if *depth_testing == is_depth_tested {
                Some((o.mesh, o.instance))
            } else {
                None
            }
        } else {
            None
        }
    });

    if is_depth_tested {
        render_pass.set_pipeline(
            render_pipeline_cache
                .get(constants::SCREEN_SPACE_WIREFRAME_PIPELINE_KEY)
                .unwrap(),
        );
    } else {
        render_pass.set_pipeline(
            render_pipeline_cache
                .get(constants::SCREEN_SPACE_WIREFRAME_WITHOUT_DEPTH_PIPELINE_KEY)
                .unwrap(),
        );
    }

    for (mesh, instance) in screen_space_objs {
        let position_buffer = mesh.vertex_slice::<vertex::Position>();
        let color_buffer = mesh.vertex_slice::<vertex::Color>();
        let transformation_buffer = instance.vertex_slice::<transformation::TransformationData>();
        let material_buffer = instance.vertex_slice::<material::RgbMaterialData>();
        let size_in_pixel_buffer = instance.vertex_slice::<screen_space::SizeInPixel>();

        render_pass.set_vertex_buffer(0, position_buffer);
        render_pass.set_vertex_buffer(1, color_buffer);
        render_pass.set_vertex_buffer(2, transformation_buffer);
        render_pass.set_vertex_buffer(3, material_buffer);
        render_pass.set_vertex_buffer(4, size_in_pixel_buffer);

        if let Some(index_stream) = &mesh.wireframe_index_stream {
            let index_buffer = mesh.buffer.slice(index_stream.offset..index_stream.end);
            render_pass.set_index_buffer(index_buffer, index_stream.format);

            render_pass.draw_indexed(0..index_stream.index_count, 0, 0..instance.instance_count);
        } else {
            render_pass.draw(0..mesh.vertex_count, 0..instance.instance_count);
        }
    }
}
