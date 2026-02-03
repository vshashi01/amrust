use crate::prelude::*;
use crate::{RenderData, Renderable, material, transformation, vertex};

use std::collections::HashMap;

pub fn solid_render_pass<'a>(
    renderables: &[RenderData<'a>],
    render_pipeline_cache: &HashMap<String, wgpu::RenderPipeline>,
    render_pass: &mut wgpu::RenderPass<'_>,
) {
    let single_textured_objects = renderables.iter().filter_map(|o| match o.renderable {
        Renderable::TexturedMesh => Some((
            o.mesh,
            o.instance,
            o.local_bind_groups.clone(),
            "Textured Surface",
        )),
        Renderable::ArrayTexturedMesh => Some((
            o.mesh,
            o.instance,
            o.local_bind_groups.clone(),
            "Texture Array Surface",
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
        render_pass.set_pipeline(render_pipeline_cache.get("Colored Surface").unwrap());
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
        render_pass.set_pipeline(render_pipeline_cache.get("Uniform Solid Surface").unwrap());
        let position_buffer = mesh.vertex_slice::<vertex::Position>();
        let transformation_buffer = instance.vertex_slice::<transformation::TransformationData>();
        let material_buffer = instance.vertex_slice::<material::RgbMaterialData>();

        render_pass.set_vertex_buffer(0, position_buffer);
        render_pass.set_vertex_buffer(1, transformation_buffer);
        render_pass.set_vertex_buffer(2, material_buffer);

        render_pass.draw(0..mesh.vertex_count, 0..instance.instance_count);
    }
}

pub fn wireframe_render_pass<'a>(
    renderables: &[RenderData<'a>],
    render_pipeline_cache: &HashMap<String, wgpu::RenderPipeline>,
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
        render_pass.set_pipeline(render_pipeline_cache.get("Wireframe").unwrap());

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
}

pub fn silhoutte_pass<'a>(
    renderables: &[RenderData<'a>],
    render_pipeline_cache: &HashMap<String, wgpu::RenderPipeline>,
    render_pass: &mut wgpu::RenderPass<'_>,
) {
    let silhoutte_objs = renderables.iter().filter_map(|o| {
        if let Renderable::OutlinedMesh { .. } = &o.renderable {
            Some((o.mesh, o.instance))
        } else {
            None
        }
    });

    for (mesh, instance) in silhoutte_objs {
        render_pass.set_pipeline(render_pipeline_cache.get("Silhoutte Surface").unwrap());
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
