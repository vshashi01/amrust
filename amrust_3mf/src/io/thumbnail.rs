use amrust_render::bounding_box::BoundingBox;
use amrust_render::camera::{CameraData, OrthographicCameraData};
use amrust_render::gpu_mesh::MeshBuilder;
use amrust_render::instance::InstanceDataBuilder;
use amrust_render::material::Material;
use amrust_render::transformation::Transformation;
use amrust_render::vertex::Position;
use amrust_render::{RenderObject, renderer};
use glam::{Mat4, Vec3};
use image::Rgba;

use crate::core::transform::Transform;
use crate::core::{Triangles, Vertices};

pub use super::ThreemfPackage;
pub use super::error::Error;

use core::f32;
use std::collections::HashMap;
use std::collections::hash_map::Entry;

struct Data {
    pub bbox: BoundingBox,
    pub transforms: Vec<Transformation>,
}

pub async fn render_package_thumbnail(
    package: &ThreemfPackage,
    width: u32,
    height: u32,
) -> Result<image::ImageBuffer<Rgba<u8>, Vec<u8>>, Error> {
    let mut renderer = renderer::Renderer::from_new_device(width, height)
        .await
        .map_err(|e| Error::ThumbnailError(e.to_string()))?;

    let mut object_transform_map = HashMap::<usize, Data>::new();
    let mut gpu_mesh_map = HashMap::new();

    for item in &package.root.build.item {
        let mesh_id = item.objectid;
        let transform = {
            if let Some(transform) = &item.transform {
                Transformation(convert_transform_to_glam_matrix(transform))
            } else {
                Transformation(glam::Mat4::IDENTITY)
            }
        };

        if let Entry::Vacant(e) = object_transform_map.entry(mesh_id) {
            //process the mesh and create a GPU mesh
            let object = package
                .root
                .resources
                .object
                .iter()
                .find(|o| o.id == mesh_id);

            if let Some(object) = object
                && let Some(mesh) = &object.mesh
            {
                let positions = convert_vertices_to_position(&mesh.vertices);
                let indices = convert_triangles_to_indices(&mesh.triangles);
                let color = convert_vertices_to_color(&mesh.vertices);
                let wireframe_indices = convert_triangles_to_wireframe_indices(&mesh.triangles);

                let gpu_mesh = MeshBuilder::new()
                    .add_vertex_stream(positions.as_slice())
                    .add_vertex_stream(color.as_slice())
                    .add_mesh_index_stream(indices.as_slice())
                    .add_wireframe_index_stream(wireframe_indices.as_slice())
                    .build(&renderer.device);

                let gpu_mesh_id = renderer.add_mesh(gpu_mesh);

                gpu_mesh_map.insert(mesh_id, gpu_mesh_id);

                //calculate bounding box
                let bbox = calculate_bounding_box(&mesh.vertices);

                e.insert(Data {
                    bbox,
                    transforms: vec![transform],
                });
            }
        } else {
            // add an additional instance transform
            object_transform_map
                .get_mut(&mesh_id)
                .unwrap()
                .transforms
                .push(transform);
        }
    }

    //create render objects
    for (mesh_id, data) in &object_transform_map {
        let transformation_data = data
            .transforms
            .iter()
            .map(|t| t.to_data())
            .collect::<Vec<_>>();
        let material_data = vec![Material::new(1.0, 1.0, 1.0).to_data(); transformation_data.len()];
        if let Some(gpu_mesh_id) = gpu_mesh_map.get(mesh_id) {
            let object = RenderObject {
                renderable: amrust_render::Renderable::ColoredMesh(*gpu_mesh_id),
                instance: InstanceDataBuilder::new()
                    .add_instance_stream(transformation_data.as_slice())
                    .add_instance_stream(material_data.as_slice())
                    .build(&renderer.device),
            };
            let _ = renderer.add_object(object);

            // println!("Colored Object id {}", object_id);

            let wireframe_object = RenderObject {
                renderable: amrust_render::Renderable::WireframeMesh(*gpu_mesh_id),
                instance: InstanceDataBuilder::new()
                    .add_instance_stream(transformation_data.as_slice())
                    .add_instance_stream(material_data.as_slice())
                    .build(&renderer.device),
            };
            let _ = renderer.add_object(wireframe_object);

            // println!("Wireframe Object id {}", wireframe_object_id);
        }
    }

    let mut total_bbox = BoundingBox {
        min: Vec3::splat(f32::MIN),
        max: Vec3::splat(f32::MIN),
    };

    for data in object_transform_map.values() {
        for transform in &data.transforms {
            let mut transformed_bbox = data.bbox.clone();
            transformed_bbox.transform(transform);
            if total_bbox.min.x == f32::MIN {
                total_bbox = transformed_bbox;
            } else {
                total_bbox.unite(&transformed_bbox);
            }
        }
    }

    //draw the total bbox
    add_bounding_box_wireframe(&mut renderer, &total_bbox);
    // println!("Bounding Box points are: {:?}", total_bbox);
    // println!("Bounding Box size is: {:?}", total_bbox.delta());
    let top_left_corner = Vec3::new(total_bbox.min.x, total_bbox.min.y, total_bbox.max.z);
    // println!("The top left corner is: {}", top_left_corner);

    let mut camera_data = OrthographicCameraData {
        eye_position: top_left_corner,
        target_position: total_bbox.center(),
        up_vector: Vec3::Z,
        ..Default::default()
    };

    // camera_data.target_position = total_bbox.center();
    // camera_data.eye_position = top_left_corner;
    // camera_data.up_vector = Vec3::Z;

    let view_matrix = camera_data.get_view_matrix();
    let max_extent = calculate_max_extent_in_camera_space(&total_bbox, &view_matrix);
    // println!("max_extent: {}", max_extent);
    if max_extent == f32::NAN {
        return Err(Error::ThumbnailError("Maximum extent is NAN".to_owned()));
    }

    camera_data.zoom = 2.0 / max_extent;

    renderer.update_camera(&camera_data);
    renderer.render().await.unwrap();

    let image_buffer = renderer.present().await;

    Ok(image_buffer)
}

fn calculate_max_extent_in_camera_space(bbox: &BoundingBox, view_matrix: &Mat4) -> f32 {
    let corners_in_camera_space = bbox.corners().map(|corner| {
        let corner_vector = corner.extend(1.0);
        let transformed = view_matrix * corner_vector;
        transformed.truncate()
    });
    let mut min = corners_in_camera_space[0];
    let mut max = corners_in_camera_space[0];

    for v in &corners_in_camera_space[1..] {
        min = min.min(*v);
        max = max.max(*v);
    }

    let extent = max - min;

    extent.x.max(extent.y).max(extent.z)
}

fn calculate_bounding_box(vertices: &Vertices) -> BoundingBox {
    let mut min = glam::Vec3::splat(f32::INFINITY);
    let mut max = glam::Vec3::splat(f32::NEG_INFINITY);

    for vertex in &vertices.vertex {
        let pos = glam::vec3(vertex.x as f32, vertex.y as f32, vertex.z as f32);
        min = min.min(pos);
        max = max.max(pos);
    }

    BoundingBox { min, max }
}

fn convert_vertices_to_position(vertices: &Vertices) -> Vec<Position> {
    vertices
        .vertex
        .iter()
        .map(|v| Position([v.x as f32, v.y as f32, v.z as f32]))
        .collect()
}

fn convert_triangles_to_indices(triangles: &Triangles) -> Vec<u32> {
    triangles
        .triangle
        .iter()
        .flat_map(|t| vec![t.v1 as u32, t.v2 as u32, t.v3 as u32])
        .collect()
}

fn convert_triangles_to_wireframe_indices(triangles: &Triangles) -> Vec<u32> {
    let mut indices = Vec::new();
    for t in &triangles.triangle {
        indices.push(t.v1 as u32);
        indices.push(t.v2 as u32);
        indices.push(t.v2 as u32);
        indices.push(t.v3 as u32);
        indices.push(t.v3 as u32);
        indices.push(t.v1 as u32);
    }

    indices
}

fn convert_vertices_to_color(vertices: &Vertices) -> Vec<amrust_render::vertex::Color> {
    vertices
        .vertex
        .iter()
        .map(|_| amrust_render::vertex::Color([0.5, 0.5, 0.5]))
        .collect()
}

fn convert_transform_to_glam_matrix(transform: &Transform) -> glam::Mat4 {
    glam::Mat4::from_cols_array_2d(&[
        [
            transform.0[0] as f32,
            transform.0[1] as f32,
            transform.0[2] as f32,
            0.0,
        ],
        [
            transform.0[3] as f32,
            transform.0[4] as f32,
            transform.0[5] as f32,
            0.0,
        ],
        [
            transform.0[6] as f32,
            transform.0[7] as f32,
            transform.0[8] as f32,
            0.0,
        ],
        [
            transform.0[9] as f32,
            transform.0[10] as f32,
            transform.0[11] as f32,
            1.0,
        ],
    ])
}

fn add_bounding_box_wireframe(
    renderer: &mut amrust_render::renderer::Renderer,
    bbox: &BoundingBox,
) -> u32 {
    let mesh = MeshBuilder::new()
        .add_vertex_stream(convert_points_vec_to_position(&bbox.corners()).as_slice())
        .add_wireframe_index_stream(&BoundingBox::wireframe_indices())
        .build(&renderer.device);

    let mesh_id = renderer.add_mesh(mesh);

    let wireframe_object = RenderObject {
        renderable: amrust_render::Renderable::WireframeMesh(mesh_id),
        instance: InstanceDataBuilder::new()
            .add_instance_stream(&[Transformation(Mat4::IDENTITY).to_data()])
            .add_instance_stream(&[Material::new(1.0, 1.0, 1.0).to_data()])
            .build(&renderer.device),
    };

    renderer.add_object(wireframe_object)
}

fn convert_points_vec_to_position(points: &[Vec3]) -> Vec<Position> {
    points.iter().map(|p| Position([p.x, p.y, p.z])).collect()
}

mod tests {
    use std::{
        cmp::Ordering,
        path::{Path, PathBuf},
    };

    fn test_data_path(rel: &str) -> PathBuf {
        let this_file = Path::new(file!());
        let dir = this_file.parent().unwrap();
        dir.join(rel)
    }

    #[test]
    fn test_thumbnail() {
        let threemf = std::fs::File::open(test_data_path(
            "../../../tests/data/third-party/meshmixer-bunny.3mf",
        ))
        .unwrap();
        let golden_thumbnail_path = test_data_path(
            "../../../tests/data/third-party/golden_thumbnails/meshmixer-bunny.3mf_golden_thumbnail.png",
        );

        let package = super::ThreemfPackage::from_reader(threemf, true).unwrap();

        const FLIP_MEAN_ERROR: f32 = 0.0;
        pollster::block_on(async {
            let ref_image_data = image::open(&golden_thumbnail_path).unwrap().into_rgba8();

            let thumbnail = super::render_package_thumbnail(&package, 1280, 1080)
                .await
                .unwrap();

            thumbnail.save("mgx-iron_giant_single.png");

            let ref_image = nv_flip::FlipImageRgb8::with_data(1280, 1080, &ref_image_data);
            let test_image = nv_flip::FlipImageRgb8::with_data(1280, 1080, &thumbnail);

            let error_map =
                nv_flip::flip(ref_image, test_image, nv_flip::DEFAULT_PIXELS_PER_DEGREE);
            let pool = nv_flip::FlipPool::from_image(&error_map);
            if let Some(Ordering::Greater) = pool.mean().partial_cmp(&FLIP_MEAN_ERROR) {
                println!("Mean error {}", pool.mean());
                let generated_thumbnail_path = golden_thumbnail_path
                    .clone()
                    .join("_generated_thumbnail.png");
                thumbnail.save(generated_thumbnail_path).unwrap();

                panic!("Something is wrong with the thumbnail");
            }
        });
    }
}
