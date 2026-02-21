use crate::{
    RenderData3d, Renderable3d, TriangleFaceMode, light, material, screen_space, transformation,
    vertex,
};
use crate::{constants, prelude::*};

use std::collections::HashMap;

/// Resolves the pipeline key based on cull mode and lit status
fn resolve_pipeline_key(
    base_key: &'static str,
    cull_mode: TriangleFaceMode,
    use_lit: bool,
) -> &'static str {
    match (base_key, cull_mode, use_lit) {
        // Textured mesh
        (constants::TEXTURE_MESH_PIPELINE_KEY, TriangleFaceMode::FrontOnly, false) => {
            constants::TEXTURE_MESH_PIPELINE_KEY
        }
        (constants::TEXTURE_MESH_PIPELINE_KEY, TriangleFaceMode::FrontOnly, true) => {
            constants::TEXTURE_MESH_PIPELINE_KEY_LIT
        }
        (constants::TEXTURE_MESH_PIPELINE_KEY, TriangleFaceMode::FrontAndBack, false) => {
            constants::NO_CULL_TEXTURE_MESH_PIPELINE_KEY
        }
        (constants::TEXTURE_MESH_PIPELINE_KEY, TriangleFaceMode::FrontAndBack, true) => {
            constants::NO_CULL_TEXTURE_MESH_PIPELINE_KEY_LIT
        }
        // Array textured mesh
        (constants::ARRAY_TEXTURE_MESH_PIPELINE_KEY, TriangleFaceMode::FrontOnly, false) => {
            constants::ARRAY_TEXTURE_MESH_PIPELINE_KEY
        }
        (constants::ARRAY_TEXTURE_MESH_PIPELINE_KEY, TriangleFaceMode::FrontOnly, true) => {
            constants::ARRAY_TEXTURE_MESH_PIPELINE_KEY_LIT
        }
        (constants::ARRAY_TEXTURE_MESH_PIPELINE_KEY, TriangleFaceMode::FrontAndBack, false) => {
            constants::NO_CULL_ARRAY_TEXTURE_MESH_PIPELINE_KEY
        }
        (constants::ARRAY_TEXTURE_MESH_PIPELINE_KEY, TriangleFaceMode::FrontAndBack, true) => {
            constants::NO_CULL_ARRAY_TEXTURE_MESH_PIPELINE_KEY_LIT
        }
        // Vertex colored mesh
        (constants::VERTEX_COLORED_MESH_PIPELINE_KEY, TriangleFaceMode::FrontOnly, false) => {
            constants::VERTEX_COLORED_MESH_PIPELINE_KEY
        }
        (constants::VERTEX_COLORED_MESH_PIPELINE_KEY, TriangleFaceMode::FrontOnly, true) => {
            constants::VERTEX_COLORED_MESH_PIPELINE_KEY_LIT
        }
        (constants::VERTEX_COLORED_MESH_PIPELINE_KEY, TriangleFaceMode::FrontAndBack, false) => {
            constants::NO_CULL_VERTEX_COLORED_MESH_PIPELINE_KEY
        }
        (constants::VERTEX_COLORED_MESH_PIPELINE_KEY, TriangleFaceMode::FrontAndBack, true) => {
            constants::NO_CULL_VERTEX_COLORED_MESH_PIPELINE_KEY_LIT
        }
        // Solid colored mesh
        (constants::SOLID_COLORED_MESH_PIPELINE_KEY, TriangleFaceMode::FrontOnly, false) => {
            constants::SOLID_COLORED_MESH_PIPELINE_KEY
        }
        (constants::SOLID_COLORED_MESH_PIPELINE_KEY, TriangleFaceMode::FrontOnly, true) => {
            constants::SOLID_COLORED_MESH_PIPELINE_KEY_LIT
        }
        (constants::SOLID_COLORED_MESH_PIPELINE_KEY, TriangleFaceMode::FrontAndBack, false) => {
            constants::NO_CULL_SOLID_COLORED_MESH_PIPELINE_KEY
        }
        (constants::SOLID_COLORED_MESH_PIPELINE_KEY, TriangleFaceMode::FrontAndBack, true) => {
            constants::NO_CULL_SOLID_COLORED_MESH_PIPELINE_KEY_LIT
        }
        // Transparent textured mesh
        (constants::TRANSPARENT_TEXTURE_MESH_PIPELINE_KEY, TriangleFaceMode::FrontOnly, false) => {
            constants::TRANSPARENT_TEXTURE_MESH_PIPELINE_KEY
        }
        (constants::TRANSPARENT_TEXTURE_MESH_PIPELINE_KEY, TriangleFaceMode::FrontOnly, true) => {
            constants::TRANSPARENT_TEXTURE_MESH_PIPELINE_KEY_LIT
        }
        (
            constants::TRANSPARENT_TEXTURE_MESH_PIPELINE_KEY,
            TriangleFaceMode::FrontAndBack,
            false,
        ) => constants::NO_CULL_TRANSPARENT_TEXTURE_MESH_PIPELINE_KEY,
        (
            constants::TRANSPARENT_TEXTURE_MESH_PIPELINE_KEY,
            TriangleFaceMode::FrontAndBack,
            true,
        ) => constants::NO_CULL_TRANSPARENT_TEXTURE_MESH_PIPELINE_KEY_LIT,
        // Transparent array textured mesh
        (
            constants::TRANSPARENT_ARRAY_TEXTURE_MESH_PIPELINE_KEY,
            TriangleFaceMode::FrontOnly,
            false,
        ) => constants::TRANSPARENT_ARRAY_TEXTURE_MESH_PIPELINE_KEY,
        (
            constants::TRANSPARENT_ARRAY_TEXTURE_MESH_PIPELINE_KEY,
            TriangleFaceMode::FrontOnly,
            true,
        ) => constants::TRANSPARENT_ARRAY_TEXTURE_MESH_PIPELINE_KEY_LIT,
        (
            constants::TRANSPARENT_ARRAY_TEXTURE_MESH_PIPELINE_KEY,
            TriangleFaceMode::FrontAndBack,
            false,
        ) => constants::NO_CULL_TRANSPARENT_ARRAY_TEXTURE_MESH_PIPELINE_KEY,
        (
            constants::TRANSPARENT_ARRAY_TEXTURE_MESH_PIPELINE_KEY,
            TriangleFaceMode::FrontAndBack,
            true,
        ) => constants::NO_CULL_TRANSPARENT_ARRAY_TEXTURE_MESH_PIPELINE_KEY_LIT,
        // Transparent vertex colored mesh
        (
            constants::TRANSPARENT_VERTEX_COLORED_MESH_PIPELINE_KEY,
            TriangleFaceMode::FrontOnly,
            false,
        ) => constants::TRANSPARENT_VERTEX_COLORED_MESH_PIPELINE_KEY,
        (
            constants::TRANSPARENT_VERTEX_COLORED_MESH_PIPELINE_KEY,
            TriangleFaceMode::FrontOnly,
            true,
        ) => constants::TRANSPARENT_VERTEX_COLORED_MESH_PIPELINE_KEY_LIT,
        (
            constants::TRANSPARENT_VERTEX_COLORED_MESH_PIPELINE_KEY,
            TriangleFaceMode::FrontAndBack,
            false,
        ) => constants::NO_CULL_TRANSPARENT_VERTEX_COLORED_MESH_PIPELINE_KEY,
        (
            constants::TRANSPARENT_VERTEX_COLORED_MESH_PIPELINE_KEY,
            TriangleFaceMode::FrontAndBack,
            true,
        ) => constants::NO_CULL_TRANSPARENT_VERTEX_COLORED_MESH_PIPELINE_KEY_LIT,
        // Transparent solid colored mesh
        (
            constants::TRANSPARENT_SOLID_COLORED_MESH_PIPELINE_KEY,
            TriangleFaceMode::FrontOnly,
            false,
        ) => constants::TRANSPARENT_SOLID_COLORED_MESH_PIPELINE_KEY,
        (
            constants::TRANSPARENT_SOLID_COLORED_MESH_PIPELINE_KEY,
            TriangleFaceMode::FrontOnly,
            true,
        ) => constants::TRANSPARENT_SOLID_COLORED_MESH_PIPELINE_KEY_LIT,
        (
            constants::TRANSPARENT_SOLID_COLORED_MESH_PIPELINE_KEY,
            TriangleFaceMode::FrontAndBack,
            false,
        ) => constants::NO_CULL_TRANSPARENT_SOLID_COLORED_MESH_PIPELINE_KEY,
        (
            constants::TRANSPARENT_SOLID_COLORED_MESH_PIPELINE_KEY,
            TriangleFaceMode::FrontAndBack,
            true,
        ) => constants::NO_CULL_TRANSPARENT_SOLID_COLORED_MESH_PIPELINE_KEY_LIT,
        // Default fallback
        _ => base_key,
    }
}

/// Resolves cull mode for a renderable: uses object's override if set, otherwise uses frame default
fn resolve_cull_mode(
    render_data: &RenderData3d,
    frame_cull_mode: TriangleFaceMode,
) -> TriangleFaceMode {
    render_data.triangle_face_mode.unwrap_or(frame_cull_mode)
}

pub fn surface_3d_render_pass_with_depth<'a>(
    renderables: &[RenderData3d<'a>],
    render_pipeline_cache: &HashMap<&'static str, wgpu::RenderPipeline>,
    render_pass: &mut wgpu::RenderPass<'_>,
    frame_cull_mode: TriangleFaceMode,
) {
    let single_textured_objects = renderables.iter().filter_map(|o| match o.renderable {
        Renderable3d::TexturedMesh => Some((
            o.mesh,
            o.instance,
            o.local_resources,
            constants::TEXTURE_MESH_PIPELINE_KEY,
            resolve_cull_mode(o, frame_cull_mode),
        )),
        Renderable3d::ArrayTexturedMesh => Some((
            o.mesh,
            o.instance,
            o.local_resources,
            constants::ARRAY_TEXTURE_MESH_PIPELINE_KEY,
            resolve_cull_mode(o, frame_cull_mode),
        )),
        _ => None,
    });

    for (mesh, instance, mesh_local, base_pipeline_key, cull_mode) in single_textured_objects {
        let use_lit = mesh.vertex_stream::<vertex::Normal>().is_some()
            && instance
                .instance_data_stream::<light::NormalMatrixData>()
                .is_some();
        let pipeline_key = resolve_pipeline_key(base_pipeline_key, cull_mode, use_lit);

        render_pass.set_pipeline(render_pipeline_cache.get(pipeline_key).unwrap());
        let position_buffer = mesh.vertex_slice::<vertex::Position3d>();
        let color_buffer = mesh.vertex_slice::<vertex::Color>();
        let tex_coord_buffer = mesh.vertex_slice::<vertex::TexCoords>();
        let use_texture_buffer = mesh.vertex_slice::<vertex::UseTexture>();
        let normal_buffer = if use_lit {
            Some(mesh.vertex_slice::<vertex::Normal>())
        } else {
            None
        };

        let transformation_buffer = instance.vertex_slice::<transformation::TransformationData>();
        let material_buffer = instance.vertex_slice::<material::RgbMaterialData>();
        let normal_matrix_buffer = if use_lit {
            Some(instance.vertex_slice::<light::NormalMatrixData>())
        } else {
            None
        };

        render_pass.set_bind_group(1, &mesh_local.uniform_bg, &[]);
        if let Some(resource_bind_group) = &mesh_local.texture_bg {
            render_pass.set_bind_group(2, resource_bind_group, &[]);
        }

        render_pass.set_vertex_buffer(0, position_buffer);
        render_pass.set_vertex_buffer(1, color_buffer);
        render_pass.set_vertex_buffer(2, tex_coord_buffer);
        render_pass.set_vertex_buffer(3, use_texture_buffer);
        if let Some(normal_buffer) = normal_buffer {
            render_pass.set_vertex_buffer(4, normal_buffer);
            render_pass.set_vertex_buffer(5, transformation_buffer);
            render_pass.set_vertex_buffer(6, material_buffer);
            render_pass.set_vertex_buffer(7, normal_matrix_buffer.unwrap());
        } else {
            render_pass.set_vertex_buffer(4, transformation_buffer);
            render_pass.set_vertex_buffer(5, material_buffer);
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
        if let Renderable3d::ColoredMesh = &o.renderable {
            Some((
                o.mesh,
                o.instance,
                &o.local_resources,
                resolve_cull_mode(o, frame_cull_mode),
            ))
        } else {
            None
        }
    });

    for (mesh, instance, mesh_local, cull_mode) in colored_objects {
        let use_lit = mesh.vertex_stream::<vertex::Normal>().is_some()
            && instance
                .instance_data_stream::<light::NormalMatrixData>()
                .is_some();
        let pipeline_key = resolve_pipeline_key(
            constants::VERTEX_COLORED_MESH_PIPELINE_KEY,
            cull_mode,
            use_lit,
        );
        render_pass.set_pipeline(render_pipeline_cache.get(pipeline_key).unwrap());
        let position_buffer = mesh.vertex_slice::<vertex::Position3d>();
        let color_buffer = mesh.vertex_slice::<vertex::Color>();
        let normal_buffer = if use_lit {
            Some(mesh.vertex_slice::<vertex::Normal>())
        } else {
            None
        };

        let transformation_buffer = instance.vertex_slice::<transformation::TransformationData>();
        let material_buffer = instance.vertex_slice::<material::RgbMaterialData>();
        let normal_matrix_buffer = if use_lit {
            Some(instance.vertex_slice::<light::NormalMatrixData>())
        } else {
            None
        };

        render_pass.set_bind_group(1, &mesh_local.uniform_bg, &[]);
        render_pass.set_vertex_buffer(0, position_buffer);
        render_pass.set_vertex_buffer(1, color_buffer);
        if let Some(normal_buffer) = normal_buffer {
            render_pass.set_vertex_buffer(2, normal_buffer);
            render_pass.set_vertex_buffer(3, transformation_buffer);
            render_pass.set_vertex_buffer(4, material_buffer);
            render_pass.set_vertex_buffer(5, normal_matrix_buffer.unwrap());
        } else {
            render_pass.set_vertex_buffer(2, transformation_buffer);
            render_pass.set_vertex_buffer(3, material_buffer);
        }

        if let Some(index_stream) = &mesh.mesh_index_stream {
            let index_buffer = mesh.buffer.slice(index_stream.offset..index_stream.end);
            render_pass.set_index_buffer(index_buffer, index_stream.format);

            render_pass.draw_indexed(0..index_stream.index_count, 0, 0..instance.instance_count);
        } else {
            panic!("Mesh does not have an index buffer");
        }
    }

    let simple_objects = renderables.iter().filter_map(|o| {
        if let Renderable3d::Mesh = &o.renderable {
            Some((
                o.mesh,
                o.instance,
                &o.local_resources,
                resolve_cull_mode(o, frame_cull_mode),
            ))
        } else {
            None
        }
    });

    for (mesh, instance, mesh_local, cull_mode) in simple_objects {
        let use_lit = mesh.vertex_stream::<vertex::Normal>().is_some()
            && instance
                .instance_data_stream::<light::NormalMatrixData>()
                .is_some();
        let pipeline_key = resolve_pipeline_key(
            constants::SOLID_COLORED_MESH_PIPELINE_KEY,
            cull_mode,
            use_lit,
        );
        render_pass.set_pipeline(render_pipeline_cache.get(pipeline_key).unwrap());
        let position_buffer = mesh.vertex_slice::<vertex::Position3d>();
        let normal_buffer = if use_lit {
            Some(mesh.vertex_slice::<vertex::Normal>())
        } else {
            None
        };
        let transformation_buffer = instance.vertex_slice::<transformation::TransformationData>();
        let material_buffer = instance.vertex_slice::<material::RgbMaterialData>();
        let normal_matrix_buffer = if use_lit {
            Some(instance.vertex_slice::<light::NormalMatrixData>())
        } else {
            None
        };

        render_pass.set_bind_group(1, &mesh_local.uniform_bg, &[]);
        render_pass.set_vertex_buffer(0, position_buffer);
        if let Some(normal_buffer) = normal_buffer {
            render_pass.set_vertex_buffer(1, normal_buffer);
            render_pass.set_vertex_buffer(2, transformation_buffer);
            render_pass.set_vertex_buffer(3, material_buffer);
            render_pass.set_vertex_buffer(4, normal_matrix_buffer.unwrap());
        } else {
            render_pass.set_vertex_buffer(1, transformation_buffer);
            render_pass.set_vertex_buffer(2, material_buffer);
        }

        render_pass.draw(0..mesh.vertex_count, 0..instance.instance_count);
    }

    screen_space_colored_mesh_pass(renderables, render_pipeline_cache, render_pass, true);
}

pub fn wireframe_3d_render_pass<'a>(
    renderables: &[RenderData3d<'a>],
    render_pipeline_cache: &HashMap<&'static str, wgpu::RenderPipeline>,
    render_pass: &mut wgpu::RenderPass<'_>,
) {
    let wireframe_objects = renderables.iter().filter_map(|o| {
        if let Renderable3d::WireframeMesh = &o.renderable {
            Some((o.mesh, o.instance, &o.local_resources))
        } else {
            None
        }
    });

    for (mesh, instance, mesh_local) in wireframe_objects {
        render_pass.set_pipeline(
            render_pipeline_cache
                .get(constants::WIREFRAME_MESH_PIPELINE_KEY)
                .unwrap(),
        );

        let position_buffer = mesh.vertex_slice::<vertex::Position3d>();
        let transformation_buffer = instance.vertex_slice::<transformation::TransformationData>();
        let material_buffer = instance.vertex_slice::<material::RgbMaterialData>();

        render_pass.set_bind_group(1, &mesh_local.uniform_bg, &[]);
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
    renderables: &[RenderData3d<'a>],
    render_pipeline_cache: &HashMap<&'static str, wgpu::RenderPipeline>,
    render_pass: &mut wgpu::RenderPass<'_>,
) {
    let silhoutte_objs = renderables.iter().filter_map(|o| {
        if let Renderable3d::SilhouetteMesh = &o.renderable {
            Some((o.mesh, o.instance, &o.local_resources))
        } else {
            None
        }
    });

    render_pass.set_pipeline(
        render_pipeline_cache
            .get(constants::MESH_SILHOUETTE_PIPELINE_KEY)
            .unwrap(),
    );

    for (mesh, instance, mesh_local) in silhoutte_objs {
        let position_buffer = mesh.vertex_slice::<vertex::Position3d>();
        let transformation_buffer = instance.vertex_slice::<transformation::TransformationData>();

        render_pass.set_bind_group(1, &mesh_local.uniform_bg, &[]);
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
    renderables: &[RenderData3d<'a>],
    render_pipeline_cache: &HashMap<&'static str, wgpu::RenderPipeline>,
    render_pass: &mut wgpu::RenderPass<'_>,
    is_depth_tested: bool,
) {
    let mut screen_space_objs = renderables
        .iter()
        .filter_map(|o| {
            if let Renderable3d::ScreenSpaceColoredMesh {
                depth_testing,
                order,
            } = &o.renderable
            {
                if *depth_testing == is_depth_tested {
                    Some((o.mesh, o.instance, order, &o.local_resources))
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

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

        screen_space_objs.sort_by_key(|(_, _, order, _)| **order);
        screen_space_objs.reverse();
    }

    for (mesh, instance, _, mesh_local) in screen_space_objs {
        let position_buffer = mesh.vertex_slice::<vertex::Position3d>();
        let color_buffer = mesh.vertex_slice::<vertex::Color>();
        let transformation_buffer = instance.vertex_slice::<transformation::TransformationData>();
        let material_buffer = instance.vertex_slice::<material::RgbMaterialData>();
        let use_material_buffer = instance.vertex_slice::<material::UseMaterialData>();
        let size_in_pixel_buffer = instance.vertex_slice::<screen_space::SizeInPixel>();

        render_pass.set_bind_group(1, &mesh_local.uniform_bg, &[]);
        render_pass.set_vertex_buffer(0, position_buffer);
        render_pass.set_vertex_buffer(1, color_buffer);
        render_pass.set_vertex_buffer(2, transformation_buffer);
        render_pass.set_vertex_buffer(3, material_buffer);
        render_pass.set_vertex_buffer(4, use_material_buffer);
        render_pass.set_vertex_buffer(5, size_in_pixel_buffer);

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
    renderables: &[RenderData3d<'a>],
    render_pipeline_cache: &HashMap<&'static str, wgpu::RenderPipeline>,
    render_pass: &mut wgpu::RenderPass<'_>,
    is_depth_tested: bool,
) {
    let mut screen_space_objs = renderables
        .iter()
        .filter_map(|o| {
            if let Renderable3d::ScreenSpaceWireframeMesh {
                depth_testing,
                order,
            } = &o.renderable
            {
                if *depth_testing == is_depth_tested {
                    Some((o.mesh, o.instance, order, &o.local_resources))
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

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

        screen_space_objs.sort_by_key(|(_, _, order, _)| **order);
        screen_space_objs.reverse();
    }

    for (mesh, instance, _, mesh_local) in screen_space_objs {
        let position_buffer = mesh.vertex_slice::<vertex::Position3d>();
        let color_buffer = mesh.vertex_slice::<vertex::Color>();
        let transformation_buffer = instance.vertex_slice::<transformation::TransformationData>();
        let material_buffer = instance.vertex_slice::<material::RgbMaterialData>();
        let use_material_buffer = instance.vertex_slice::<material::UseMaterialData>();
        let size_in_pixel_buffer = instance.vertex_slice::<screen_space::SizeInPixel>();

        render_pass.set_bind_group(1, &mesh_local.uniform_bg, &[]);
        render_pass.set_vertex_buffer(0, position_buffer);
        render_pass.set_vertex_buffer(1, color_buffer);
        render_pass.set_vertex_buffer(2, transformation_buffer);
        render_pass.set_vertex_buffer(3, material_buffer);
        render_pass.set_vertex_buffer(4, use_material_buffer);
        render_pass.set_vertex_buffer(5, size_in_pixel_buffer);

        if let Some(index_stream) = &mesh.wireframe_index_stream {
            let index_buffer = mesh.buffer.slice(index_stream.offset..index_stream.end);
            render_pass.set_index_buffer(index_buffer, index_stream.format);

            render_pass.draw_indexed(0..index_stream.index_count, 0, 0..instance.instance_count);
        } else {
            render_pass.draw(0..mesh.vertex_count, 0..instance.instance_count);
        }
    }
}

pub fn transparent_mesh_pass<'a>(
    renderables: &[RenderData3d<'a>],
    render_pipeline_cache: &HashMap<&'static str, wgpu::RenderPipeline>,
    render_pass: &mut wgpu::RenderPass<'_>,
    frame_cull_mode: TriangleFaceMode,
) {
    // Filter and collect transparent renderables, sorted by z_order (back-to-front)
    let mut transparent_renderables: Vec<_> = renderables
        .iter()
        .filter(|o| o.local_resources.transparency.enabled)
        .collect();

    // Sort by z_order descending (higher values render last, on top)
    transparent_renderables
        .sort_by_key(|o| std::cmp::Reverse(o.local_resources.transparency.z_order));

    // Render textured meshes
    let textured_objects = transparent_renderables
        .iter()
        .filter_map(|o| match o.renderable {
            Renderable3d::TexturedMesh => Some((
                o.mesh,
                o.instance,
                o.local_resources,
                constants::TRANSPARENT_TEXTURE_MESH_PIPELINE_KEY,
                resolve_cull_mode(o, frame_cull_mode),
            )),
            Renderable3d::ArrayTexturedMesh => Some((
                o.mesh,
                o.instance,
                o.local_resources,
                constants::TRANSPARENT_ARRAY_TEXTURE_MESH_PIPELINE_KEY,
                resolve_cull_mode(o, frame_cull_mode),
            )),
            _ => None,
        });

    for (mesh, instance, mesh_local, base_pipeline_key, cull_mode) in textured_objects {
        let use_lit = mesh.vertex_stream::<vertex::Normal>().is_some()
            && instance
                .instance_data_stream::<light::NormalMatrixData>()
                .is_some();
        let pipeline_key = resolve_pipeline_key(base_pipeline_key, cull_mode, use_lit);

        render_pass.set_pipeline(render_pipeline_cache.get(pipeline_key).unwrap());
        let position_buffer = mesh.vertex_slice::<vertex::Position3d>();
        let color_buffer = mesh.vertex_slice::<vertex::Color>();
        let tex_coord_buffer = mesh.vertex_slice::<vertex::TexCoords>();
        let use_texture_buffer = mesh.vertex_slice::<vertex::UseTexture>();
        let normal_buffer = if use_lit {
            Some(mesh.vertex_slice::<vertex::Normal>())
        } else {
            None
        };

        let transformation_buffer = instance.vertex_slice::<transformation::TransformationData>();
        let material_buffer = instance.vertex_slice::<material::RgbMaterialData>();
        let normal_matrix_buffer = if use_lit {
            Some(instance.vertex_slice::<light::NormalMatrixData>())
        } else {
            None
        };

        render_pass.set_bind_group(1, &mesh_local.uniform_bg, &[]);
        if let Some(resource_bind_group) = &mesh_local.texture_bg {
            render_pass.set_bind_group(2, resource_bind_group, &[]);
        }

        render_pass.set_vertex_buffer(0, position_buffer);
        render_pass.set_vertex_buffer(1, color_buffer);
        render_pass.set_vertex_buffer(2, tex_coord_buffer);
        render_pass.set_vertex_buffer(3, use_texture_buffer);
        if let Some(normal_buffer) = normal_buffer {
            render_pass.set_vertex_buffer(4, normal_buffer);
            render_pass.set_vertex_buffer(5, transformation_buffer);
            render_pass.set_vertex_buffer(6, material_buffer);
            render_pass.set_vertex_buffer(7, normal_matrix_buffer.unwrap());
        } else {
            render_pass.set_vertex_buffer(4, transformation_buffer);
            render_pass.set_vertex_buffer(5, material_buffer);
        }

        if let Some(index_stream) = &mesh.mesh_index_stream {
            let index_buffer = mesh.buffer.slice(index_stream.offset..index_stream.end);
            render_pass.set_index_buffer(index_buffer, index_stream.format);
            render_pass.draw_indexed(0..index_stream.index_count, 0, 0..instance.instance_count);
        } else {
            panic!("Mesh does not have an index buffer");
        }
    }

    // Render colored meshes
    let colored_objects = transparent_renderables.iter().filter_map(|o| {
        if let Renderable3d::ColoredMesh = &o.renderable {
            Some((
                o.mesh,
                o.instance,
                &o.local_resources,
                resolve_cull_mode(o, frame_cull_mode),
            ))
        } else {
            None
        }
    });

    for (mesh, instance, mesh_local, cull_mode) in colored_objects {
        let use_lit = mesh.vertex_stream::<vertex::Normal>().is_some()
            && instance
                .instance_data_stream::<light::NormalMatrixData>()
                .is_some();
        let pipeline_key = resolve_pipeline_key(
            constants::TRANSPARENT_VERTEX_COLORED_MESH_PIPELINE_KEY,
            cull_mode,
            use_lit,
        );
        render_pass.set_pipeline(render_pipeline_cache.get(pipeline_key).unwrap());
        let position_buffer = mesh.vertex_slice::<vertex::Position3d>();
        let color_buffer = mesh.vertex_slice::<vertex::Color>();
        let normal_buffer = if use_lit {
            Some(mesh.vertex_slice::<vertex::Normal>())
        } else {
            None
        };

        let transformation_buffer = instance.vertex_slice::<transformation::TransformationData>();
        let material_buffer = instance.vertex_slice::<material::RgbMaterialData>();
        let normal_matrix_buffer = if use_lit {
            Some(instance.vertex_slice::<light::NormalMatrixData>())
        } else {
            None
        };

        render_pass.set_bind_group(1, &mesh_local.uniform_bg, &[]);
        render_pass.set_vertex_buffer(0, position_buffer);
        render_pass.set_vertex_buffer(1, color_buffer);
        if let Some(normal_buffer) = normal_buffer {
            render_pass.set_vertex_buffer(2, normal_buffer);
            render_pass.set_vertex_buffer(3, transformation_buffer);
            render_pass.set_vertex_buffer(4, material_buffer);
            render_pass.set_vertex_buffer(5, normal_matrix_buffer.unwrap());
        } else {
            render_pass.set_vertex_buffer(2, transformation_buffer);
            render_pass.set_vertex_buffer(3, material_buffer);
        }

        if let Some(index_stream) = &mesh.mesh_index_stream {
            let index_buffer = mesh.buffer.slice(index_stream.offset..index_stream.end);
            render_pass.set_index_buffer(index_buffer, index_stream.format);
            render_pass.draw_indexed(0..index_stream.index_count, 0, 0..instance.instance_count);
        } else {
            panic!("Mesh does not have an index buffer");
        }
    }

    // Render solid/uniform colored meshes
    let solid_objects = transparent_renderables.iter().filter_map(|o| {
        if let Renderable3d::Mesh = &o.renderable {
            Some((
                o.mesh,
                o.instance,
                &o.local_resources,
                resolve_cull_mode(o, frame_cull_mode),
            ))
        } else {
            None
        }
    });

    for (mesh, instance, mesh_local, cull_mode) in solid_objects {
        let use_lit = mesh.vertex_stream::<vertex::Normal>().is_some()
            && instance
                .instance_data_stream::<light::NormalMatrixData>()
                .is_some();
        let pipeline_key = resolve_pipeline_key(
            constants::TRANSPARENT_SOLID_COLORED_MESH_PIPELINE_KEY,
            cull_mode,
            use_lit,
        );
        render_pass.set_pipeline(render_pipeline_cache.get(pipeline_key).unwrap());
        let position_buffer = mesh.vertex_slice::<vertex::Position3d>();
        let normal_buffer = if use_lit {
            Some(mesh.vertex_slice::<vertex::Normal>())
        } else {
            None
        };
        let transformation_buffer = instance.vertex_slice::<transformation::TransformationData>();
        let material_buffer = instance.vertex_slice::<material::RgbMaterialData>();
        let normal_matrix_buffer = if use_lit {
            Some(instance.vertex_slice::<light::NormalMatrixData>())
        } else {
            None
        };

        render_pass.set_bind_group(1, &mesh_local.uniform_bg, &[]);
        render_pass.set_vertex_buffer(0, position_buffer);
        if let Some(normal_buffer) = normal_buffer {
            render_pass.set_vertex_buffer(1, normal_buffer);
            render_pass.set_vertex_buffer(2, transformation_buffer);
            render_pass.set_vertex_buffer(3, material_buffer);
            render_pass.set_vertex_buffer(4, normal_matrix_buffer.unwrap());
        } else {
            render_pass.set_vertex_buffer(1, transformation_buffer);
            render_pass.set_vertex_buffer(2, material_buffer);
        }

        render_pass.draw(0..mesh.vertex_count, 0..instance.instance_count);
    }

    // Render wireframe meshes
    let wireframe_objects = transparent_renderables.iter().filter_map(|o| {
        if let Renderable3d::WireframeMesh = &o.renderable {
            Some((o.mesh, o.instance, &o.local_resources))
        } else {
            None
        }
    });

    for (mesh, instance, mesh_local) in wireframe_objects {
        render_pass.set_pipeline(
            render_pipeline_cache
                .get(constants::TRANSPARENT_WIREFRAME_MESH_PIPELINE_KEY)
                .unwrap(),
        );

        let position_buffer = mesh.vertex_slice::<vertex::Position3d>();
        let transformation_buffer = instance.vertex_slice::<transformation::TransformationData>();
        let material_buffer = instance.vertex_slice::<material::RgbMaterialData>();

        render_pass.set_bind_group(1, &mesh_local.uniform_bg, &[]);
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
