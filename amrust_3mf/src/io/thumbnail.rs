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
use thiserror::Error;

use crate::core::component::Components;
use crate::core::transform::Transform;
use crate::core::{Mesh, Triangles, Vertex};
use crate::query::get_object_ref_from_id;

pub use super::ThreemfPackage;
pub use super::error::Error;

use core::f32;
use std::collections::HashMap;

type ObjectId = usize;

enum PartRep {
    Mesh {
        vertices: Vec<Vec3>,
        triangles: Vec<u32>,
    },
    ComposedPart(Vec<ObjectId>),
}

struct Data {
    pub part_rep: PartRep, //optional data to gpu data in renderer
    pub transforms: Vec<Transformation>,
}

#[derive(Debug, Error)]
pub enum DbFrom3mfError {
    #[error("Object not found: {0}")]
    ObjectNotFound(usize),

    #[error("The Composed parts is empty {0}")]
    EmptyComposedPart(usize),

    #[error("Object does not contain data {0}")]
    EmptyObject(usize),
}

struct RenderDb {
    object_id_to_repdata: HashMap<usize, Data>,
    object_id_to_gpudata: HashMap<usize, u32>, //mesh object id to gpu_mesh_id
}

fn process_object_and_register_part_rep_data(
    db: &mut RenderDb,
    object_id: usize,
    package: &ThreemfPackage,
    renderer: &mut renderer::Renderer,
    parent_transformation: &Transformation,
    path: &Option<String>,
    parent_model: &Option<String>,
) -> Result<usize, DbFrom3mfError> {
    if let Some(data) = db.object_id_to_repdata.get_mut(&object_id) {
        data.transforms.push(*parent_transformation);
        Ok(object_id)
    } else {
        let (object, parent_model_path) =
            get_object_ref_from_id(object_id, package, &path, &parent_model);

        match object {
            Some(object) => {
                if let Some(m) = &object.mesh {
                    process_mesh_object(db, object.id, renderer, m, parent_transformation)
                } else if let Some(comps) = &object.components {
                    process_composed_object(
                        db,
                        object.id,
                        comps,
                        package,
                        renderer,
                        parent_transformation,
                        &parent_model_path,
                    )
                } else {
                    Err(DbFrom3mfError::EmptyObject(object_id))
                }
            }
            None => Err(DbFrom3mfError::ObjectNotFound(object_id)),
        }
    }
}

fn process_mesh_object(
    db: &mut RenderDb,
    object_id: usize,
    renderer: &mut renderer::Renderer,
    m: &Mesh,
    parent_transformation: &Transformation,
) -> Result<usize, DbFrom3mfError> {
    let vertices = convert_3mf_vertices_to_mesh_vertices(&m.vertices.vertex);
    let triangles = convert_triangles_to_indices(&m.triangles);

    let gpu_data = create_gpu_data(renderer, &vertices, &triangles);

    let mesh = PartRep::Mesh {
        vertices,
        triangles,
    };

    db.object_id_to_repdata.insert(
        object_id,
        Data {
            part_rep: mesh,
            transforms: vec![*parent_transformation],
        },
    );

    db.object_id_to_gpudata.insert(object_id, gpu_data);

    Ok(object_id)
}

fn process_composed_object(
    db: &mut RenderDb,
    object_id: usize,
    comps: &Components,
    package: &ThreemfPackage,
    renderer: &mut renderer::Renderer,
    parent_transformation: &Transformation,
    parent_model: &Option<String>,
) -> Result<usize, DbFrom3mfError> {
    // let mut list_of_unique_part_id_per_component = vec![];
    let mut components = vec![];
    for comp in &comps.component {
        let transform = get_transformation(&comp.transform);
        let combined_transform = Transformation(parent_transformation.0 * transform.0);

        let id = process_object_and_register_part_rep_data(
            db,
            comp.objectid,
            package,
            renderer,
            &combined_transform,
            &comp.path,
            parent_model,
        )?;

        components.push(id);
    }

    db.object_id_to_repdata.insert(
        object_id,
        Data {
            part_rep: PartRep::ComposedPart(components),
            transforms: vec![*parent_transformation],
        },
    );

    Ok(object_id)
}

pub async fn render_package_thumbnail(
    package: &ThreemfPackage,
    width: u32,
    height: u32,
) -> Result<image::ImageBuffer<Rgba<u8>, Vec<u8>>, Error> {
    let mut renderer = renderer::Renderer::from_new_device(width, height)
        .await
        .map_err(|e| Error::ThumbnailError(e.to_string()))?;

    let mut db = RenderDb {
        object_id_to_repdata: HashMap::new(),
        object_id_to_gpudata: HashMap::new(),
    };

    let mut item_we_care_about = vec![];

    for item in &package.root.build.item {
        let transform = {
            if let Some(transform) = &item.transform {
                Transformation(convert_transform_to_glam_matrix(transform))
            } else {
                Transformation(glam::Mat4::IDENTITY)
            }
        };

        let id = process_object_and_register_part_rep_data(
            &mut db,
            item.objectid,
            package,
            &mut renderer,
            &transform,
            &None,
            &None,
        )?;

        item_we_care_about.push(id);
    }

    let mut total_bbox = BoundingBox::default();

    //match instance data to the gpu data
    for (id, data) in &db.object_id_to_repdata {
        match &data.part_rep {
            PartRep::Mesh {
                vertices,
                triangles: _,
            } => {
                if let Some(gpu_mesh_id) = db.object_id_to_gpudata.get(id) {
                    add_mesh_render_object(&mut renderer, *gpu_mesh_id, &data.transforms);
                    for transform in &data.transforms {
                        let bbox = compute_transformed_bounding_box_from_mesh(vertices, transform);
                        total_bbox.unite(&bbox);
                    }
                }
            }
            PartRep::ComposedPart(items) => {
                for transform in &data.transforms {
                    process_composed_part_items(
                        &mut renderer,
                        &db,
                        id,
                        items,
                        &mut total_bbox,
                        transform,
                    );
                }
            }
        }
    }

    add_bounding_box_wireframe(&mut renderer, &total_bbox);

    let top_left_corner = Vec3::new(total_bbox.min.x, total_bbox.min.y, total_bbox.max.z);
    let mut camera_data = OrthographicCameraData {
        eye_position: top_left_corner,
        target_position: total_bbox.center(),
        up_vector: Vec3::Z,
        ..Default::default()
    };

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

fn process_composed_part_items(
    renderer: &mut renderer::Renderer,
    db: &RenderDb,
    id: &usize,
    items: &Vec<usize>,
    bbox: &mut BoundingBox,
    parent_transformation: &Transformation,
) {
    for item in items {
        if let Some(data) = &db.object_id_to_repdata.get(item) {
            match &data.part_rep {
                PartRep::Mesh {
                    vertices,
                    triangles: _,
                } => {
                    if let Some(gpu_mesh_id) = db.object_id_to_gpudata.get(id) {
                        add_mesh_render_object(renderer, *gpu_mesh_id, &data.transforms);
                        let part_bbox = compute_transformed_bounding_box_from_mesh(
                            vertices,
                            parent_transformation,
                        );
                        bbox.unite(&part_bbox);
                    }
                }
                PartRep::ComposedPart(items) => {
                    process_composed_part_items(
                        renderer,
                        db,
                        id,
                        items,
                        bbox,
                        data.transforms.first().unwrap(),
                    );
                }
            }
        }
    }
}

fn add_mesh_render_object(
    renderer: &mut renderer::Renderer,
    gpu_mesh_id: u32,
    transforms: &[Transformation],
) {
    let transformation_data = transforms.iter().map(|t| t.to_data()).collect::<Vec<_>>();
    let material_data = vec![Material::new(1.0, 1.0, 1.0).to_data(); transformation_data.len()];
    let object = RenderObject {
        renderable: amrust_render::Renderable::ColoredMesh(gpu_mesh_id),
        instance: InstanceDataBuilder::new()
            .add_instance_stream(transformation_data.as_slice())
            .add_instance_stream(material_data.as_slice())
            .build(&renderer.device),
    };
    let _ = renderer.add_object(object);

    // println!("Colored Object id {}", object_id);

    let wireframe_object = RenderObject {
        renderable: amrust_render::Renderable::WireframeMesh(gpu_mesh_id),
        instance: InstanceDataBuilder::new()
            .add_instance_stream(transformation_data.as_slice())
            .add_instance_stream(material_data.as_slice())
            .build(&renderer.device),
    };
    let _ = renderer.add_object(wireframe_object);

    // println!("Wireframe Object id {}", wireframe_object_id);
}

fn create_gpu_data(renderer: &mut renderer::Renderer, vertices: &[Vec3], triangles: &[u32]) -> u32 {
    let positions = convert_vertices_to_position(vertices);
    let indices = triangles;
    let color = convert_vertices_to_color(vertices);
    let wireframe_indices = convert_triangles_to_wireframe_indices(triangles);

    let gpu_mesh = MeshBuilder::new()
        .add_vertex_stream(positions.as_slice())
        .add_vertex_stream(color.as_slice())
        .add_mesh_index_stream(indices)
        .add_wireframe_index_stream(wireframe_indices.as_slice())
        .build(&renderer.device);

    renderer.add_mesh(gpu_mesh)
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

fn get_transformation(transform: &Option<Transform>) -> Transformation {
    {
        if let Some(transform) = transform {
            Transformation(convert_transform_to_glam_matrix(transform))
        } else {
            Transformation(glam::Mat4::IDENTITY)
        }
    }
}

fn convert_3mf_vertices_to_mesh_vertices(vertices: &[Vertex]) -> Vec<Vec3> {
    vertices
        .iter()
        .map(|v| Vec3::new(v.x as f32, v.y as f32, v.z as f32))
        .collect()
}

fn convert_vertices_to_position(vertices: &[Vec3]) -> Vec<Position> {
    vertices.iter().map(|v| Position([v.x, v.y, v.z])).collect()
}

fn convert_triangles_to_indices(triangles: &Triangles) -> Vec<u32> {
    triangles
        .triangle
        .iter()
        .flat_map(|t| vec![t.v1 as u32, t.v2 as u32, t.v3 as u32])
        .collect()
}

fn convert_triangles_to_wireframe_indices(triangles: &[u32]) -> Vec<u32> {
    let mut indices = Vec::new();
    for t in triangles.chunks(3) {
        indices.push(t[0]);
        indices.push(t[1]);
        indices.push(t[1]);
        indices.push(t[2]);
        indices.push(t[2]);
        indices.push(t[0]);
    }

    indices
}

fn convert_vertices_to_color(vertices: &[Vec3]) -> Vec<amrust_render::vertex::Color> {
    vertices
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

fn compute_transformed_bounding_box_from_mesh(
    vertices: &Vec<Vec3>,
    transform: &Transformation,
) -> BoundingBox {
    let mut bbox = BoundingBox::default();
    for v in vertices {
        let v4 = transform.0 * v.extend(1.0);
        let transformed = Vec3::new(v4.x, v4.y, v4.z);
        bbox.expand_to_include(&transformed);
    }
    bbox
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

        const FLIP_MEAN_ERROR: f32 = 0.021;
        pollster::block_on(async {
            let ref_image_data = image::open(&golden_thumbnail_path).unwrap().into_rgba8();

            let thumbnail = super::render_package_thumbnail(&package, 1280, 1080)
                .await
                .unwrap();

            let ref_image = nv_flip::FlipImageRgb8::with_data(1280, 1080, &ref_image_data);
            let test_image = nv_flip::FlipImageRgb8::with_data(1280, 1080, &thumbnail);

            let error_map =
                nv_flip::flip(ref_image, test_image, nv_flip::DEFAULT_PIXELS_PER_DEGREE);
            let pool = nv_flip::FlipPool::from_image(&error_map);
            if let Some(Ordering::Greater) = pool.mean().partial_cmp(&FLIP_MEAN_ERROR) {
                println!("Mean error {}", pool.mean());
                thumbnail
                    .save(format!("meshmixer-bunny.3mf_golden_thumbnail.png"))
                    .unwrap();

                panic!("Something is wrong with the thumbnail");
            }
        });
    }
}
