use crate::{
    gpu_mesh::GpuMesh, material, object::RenderObject, renderables::Renderable, transformation,
    vertex,
};

use std::collections::HashMap;

pub fn solid_render_pass(
    objects: &[RenderObject],
    meshes: &[GpuMesh],
    local_bind_groups: &[wgpu::BindGroup],
    render_pipeline_cache: &HashMap<String, wgpu::RenderPipeline>,
    render_pass: &mut wgpu::RenderPass<'_>,
) {
    let single_textured_objects = objects.iter().filter_map(|o| match &o.renderable {
        Renderable::TexturedMesh(mesh_id, local_bind_groups_list) => Some((
            mesh_id,
            &o.instance,
            local_bind_groups_list,
            "Textured Surface",
        )),
        Renderable::ArrayTexturedMesh(mesh_id, local_bind_groups_list) => Some((
            mesh_id,
            &o.instance,
            local_bind_groups_list,
            "Texture Array Surface",
        )),
        _ => None,
    });

    for (mesh_id, instance, bind_groups_list, pipeline_key) in single_textured_objects {
        let mesh = meshes.get(*mesh_id as usize).unwrap();
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
            let local_bind_group = local_bind_groups.get(pair.0 as usize).unwrap();
            render_pass.set_bind_group(pair.1, local_bind_group, &[]);
        }

        if let Some(index_stream) = &mesh.index_stream {
            let index_buffer = mesh.buffer.slice(index_stream.offset..index_stream.end);
            render_pass.set_index_buffer(index_buffer, index_stream.format);

            render_pass.draw_indexed(0..index_stream.index_count, 0, 0..instance.instance_count);
        } else {
            panic!("Mesh does not have an index buffer");
        }
    }

    let colored_objects = objects.iter().filter_map(|o| {
        if let Renderable::ColoredMesh(mesh_id) = &o.renderable {
            Some((mesh_id, &o.instance))
        } else {
            None
        }
    });

    for (mesh_id, instance) in colored_objects {
        let mesh = meshes.get(*mesh_id as usize).unwrap();
        render_pass.set_pipeline(render_pipeline_cache.get("Colored Surface").unwrap());
        let position_buffer = mesh.vertex_slice::<vertex::Position>();
        let color_buffer = mesh.vertex_slice::<vertex::Color>();

        let transformation_buffer = instance.vertex_slice::<transformation::TransformationData>();
        let material_buffer = instance.vertex_slice::<material::RgbMaterialData>();

        render_pass.set_vertex_buffer(0, position_buffer);
        render_pass.set_vertex_buffer(1, color_buffer);
        render_pass.set_vertex_buffer(2, transformation_buffer);
        render_pass.set_vertex_buffer(3, material_buffer);

        if let Some(index_stream) = &mesh.index_stream {
            let index_buffer = mesh.buffer.slice(index_stream.offset..index_stream.end);
            render_pass.set_index_buffer(index_buffer, index_stream.format);

            render_pass.draw_indexed(0..index_stream.index_count, 0, 0..instance.instance_count);
        } else {
            panic!("Mesh does not have an index buffer");
        }
    }

    let simple_objects = objects.iter().filter_map(|o| {
        if let Renderable::Mesh(mesh_id) = &o.renderable {
            Some((mesh_id, &o.instance))
        } else {
            None
        }
    });

    for (meshid, instance) in simple_objects {
        let mesh = meshes.get(*meshid as usize).unwrap();
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

pub fn wireframe_render_pass(
    objects: &[RenderObject],
    meshes: &[GpuMesh],
    local_bind_groups: &[wgpu::BindGroup],
    render_pipeline_cache: &HashMap<String, wgpu::RenderPipeline>,
    render_pass: &mut wgpu::RenderPass<'_>,
) {
    let _ = local_bind_groups;
    let wireframe_objects = objects.iter().filter_map(|o| {
        if let Renderable::WireframeMesh(mesh_id) = &o.renderable {
            Some((mesh_id, &o.instance))
        } else {
            None
        }
    });

    for (mesh_id, instance) in wireframe_objects {
        let mesh = meshes.get(*mesh_id as usize).unwrap();
        render_pass.set_pipeline(render_pipeline_cache.get("Wireframe").unwrap());

        let position_buffer = mesh.vertex_slice::<vertex::Position>();
        let transformation_buffer = instance.vertex_slice::<transformation::TransformationData>();
        let material_buffer = instance.vertex_slice::<material::RgbMaterialData>();

        render_pass.set_vertex_buffer(0, position_buffer);
        render_pass.set_vertex_buffer(1, transformation_buffer);
        render_pass.set_vertex_buffer(2, material_buffer);

        if let Some(index_stream) = &mesh.index_stream {
            let index_buffer = mesh.buffer.slice(index_stream.offset..index_stream.end);
            render_pass.set_index_buffer(index_buffer, index_stream.format);

            render_pass.draw_indexed(0..index_stream.index_count, 0, 0..instance.instance_count);
        } else {
            render_pass.draw(0..mesh.vertex_count, 0..instance.instance_count);
        }
    }
}
