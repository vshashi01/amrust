use glam::{Mat4, Vec3};
use thiserror::Error;

use amrust_3mf::core::transform::Transform;
use amrust_3mf::core::{Triangle, Vertex};
use amrust_3mf::io::ThreemfPackage;
use amrust_render::bounding_box::BoundingBox;
use amrust_render::transformation::Transformation;

use crate::amrust_db::{Db, Mesh, PartInstance, PartRep, Scene};

use core::f32;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Error)]
pub enum DbFrom3mfError {
    #[error("Something went wrong: {0}")]
    Unspecified(String),
}

pub fn get_db_from_3mf(filepath: PathBuf) -> Result<Db, DbFrom3mfError> {
    let threemf = std::fs::File::open(filepath).unwrap();
    let package = ThreemfPackage::from_reader(threemf, true).unwrap();

    let mut db = Db::new();
    let mut items_transform_pair = vec![];
    let mut part_rep_map = HashMap::<usize, (usize, usize)>::new(); //object id to (part_red_id, unique_part_id)
    let mut parts_in_scene = vec![];

    //collect all the object ids that do exist in the scene
    for item in &package.root.build.item {
        let object_id = item.objectid;
        let transform = {
            if let Some(transform) = &item.transform {
                Transformation(convert_transform_to_glam_matrix(transform))
            } else {
                Transformation(glam::Mat4::IDENTITY)
            }
        };

        items_transform_pair.push((object_id, transform));
    }

    //setup the Mesh, PartRep and Part based on the items on the scene
    //what to do with objects that are not listed in the build?
    //will the order of processing be objects always go such that all Components of a Composed Part
    //is already a valid Part
    for item in items_transform_pair {
        if let Some(ids) = part_rep_map.get_key_value(&item.0) {
            parts_in_scene.push(PartInstance {
                part_id: ids.1.1, //set the unique part id
                transform: item.1,
            });
        } else {
            let object = package
                .root
                .resources
                .object
                .iter()
                .find(|o| o.id == item.0);

            match object {
                Some(object) => {
                    let unique_part_id = {
                        if let Some(m) = &object.mesh {
                            process_mesh_object(&mut db, &mut part_rep_map, object.id, m)
                        } else if let Some(comps) = &object.components {
                            process_composed_object(&mut db, &mut part_rep_map, object.id, comps)
                        } else {
                            None
                        }
                    };

                    match unique_part_id {
                        Some(part_id) => {
                            parts_in_scene.push(PartInstance {
                                part_id,
                                transform: item.1,
                            });
                        }
                        None => {
                            return Err(DbFrom3mfError::Unspecified(format!(
                                "Invalid 3mf Object Id: {}",
                                item.0
                            )));
                        }
                    }
                }
                None => {
                    return Err(DbFrom3mfError::Unspecified(format!(
                        "Invalid 3mf Object Id: {}",
                        item.0
                    )));
                }
            }
        }
    }

    db.add_scene(Scene(parts_in_scene));

    Ok(db)
}

fn process_mesh_object(
    db: &mut Db,
    part_rep_map: &mut HashMap<usize, (usize, usize)>,
    object_id: usize,
    m: &amrust_3mf::core::Mesh,
) -> Option<usize> {
    let mesh = Mesh {
        vertices: convert_3mf_vertices_to_mesh_vertices(&m.vertices.vertex),
        triangles: convert_3mf_triangles_to_mesh_triangles(&m.triangles.triangle),
        bbox: generate_bbox(&m.vertices.vertex),
    };

    if let Ok(registered) = db.add_part_rep(PartRep::Mesh(Box::new(mesh))) {
        part_rep_map.insert(object_id, registered);
        Some(registered.1)
    } else {
        None
    }
}

fn process_composed_object(
    db: &mut Db,
    part_rep_map: &mut HashMap<usize, (usize, usize)>,
    object_id: usize,
    comps: &amrust_3mf::core::component::Components,
) -> Option<usize> {
    let mut list_of_unique_part_id_per_component = vec![];
    for comp in &comps.component {
        if let Some(partids) = part_rep_map.get(&comp.objectid) {
            list_of_unique_part_id_per_component.push(PartInstance {
                part_id: partids.1,
                transform: match &comp.transform {
                    Some(transform) => Transformation(convert_transform_to_glam_matrix(transform)),
                    None => Transformation(Mat4::IDENTITY),
                },
            });
        } //else need to look at other objects
    }

    if !list_of_unique_part_id_per_component.is_empty()
        && let Ok(registered) =
            db.add_part_rep(PartRep::ComposedPart(list_of_unique_part_id_per_component))
    {
        part_rep_map.insert(object_id, registered);

        return Some(registered.1);
    }
    None
}

fn generate_bbox(vertices: &[Vertex]) -> BoundingBox {
    let mut min = glam::Vec3::splat(f32::INFINITY);
    let mut max = glam::Vec3::splat(f32::NEG_INFINITY);

    for vertex in vertices {
        let pos = glam::vec3(vertex.x as f32, vertex.y as f32, vertex.z as f32);
        min = min.min(pos);
        max = max.max(pos);
    }

    BoundingBox { min, max }
}

fn convert_3mf_triangles_to_mesh_triangles(triangles: &[Triangle]) -> Vec<u32> {
    triangles
        .iter()
        .flat_map(|t| vec![t.v1 as u32, t.v2 as u32, t.v3 as u32])
        .collect()
}

fn convert_3mf_vertices_to_mesh_vertices(vertices: &[Vertex]) -> Vec<Vec3> {
    vertices
        .iter()
        .map(|v| Vec3::new(v.x as f32, v.y as f32, v.z as f32))
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

// struct Data {
//     pub bbox: BoundingBox,
//     pub transforms: Vec<Transformation>,
// }

// pub fn add_mesh_from_3mf(
//     renderer: &mut renderer::Renderer,
//     filepath: PathBuf,
//     camera: &mut impl CameraData,
// ) -> BoundingBox {
//     let threemf = std::fs::File::open(filepath).unwrap();
//     let package = ThreemfPackage::from_reader(threemf, true).unwrap();

//     let mut object_transform_map = HashMap::<usize, Data>::new();
//     let mut gpu_mesh_map = HashMap::new();

//     for item in &package.root.build.item {
//         let object_id = item.objectid;
//         let transform = {
//             if let Some(transform) = &item.transform {
//                 Transformation(convert_transform_to_glam_matrix(transform))
//             } else {
//                 Transformation(glam::Mat4::IDENTITY)
//             }
//         };

//         process_object_id(
//             renderer,
//             &package,
//             &mut object_transform_map,
//             &mut gpu_mesh_map,
//             object_id,
//             &transform,
//         );
//     }

//     //create render objects
//     for (mesh_id, data) in &object_transform_map {
//         let transformation_data = data
//             .transforms
//             .iter()
//             .map(|t| t.to_data())
//             .collect::<Vec<_>>();
//         let material_data = vec![Material::new(1.0, 1.0, 1.0).to_data(); transformation_data.len()];
//         if let Some(gpu_mesh_id) = gpu_mesh_map.get(mesh_id) {
//             let object = RenderObject {
//                 renderable: amrust_render::Renderable::ColoredMesh(*gpu_mesh_id),
//                 instance: InstanceDataBuilder::new()
//                     .add_instance_stream(transformation_data.as_slice())
//                     .add_instance_stream(material_data.as_slice())
//                     .build(&renderer.device),
//             };
//             let _ = renderer.add_object(object);

//             // println!("Colored Object id {}", object_id);

//             let wireframe_object = RenderObject {
//                 renderable: amrust_render::Renderable::WireframeMesh(*gpu_mesh_id),
//                 instance: InstanceDataBuilder::new()
//                     .add_instance_stream(transformation_data.as_slice())
//                     .add_instance_stream(material_data.as_slice())
//                     .build(&renderer.device),
//             };
//             let _ = renderer.add_object(wireframe_object);

//             // println!("Wireframe Object id {}", wireframe_object_id);
//         }
//     }

//     let mut total_bbox = BoundingBox {
//         min: Vec3::splat(f32::MIN),
//         max: Vec3::splat(f32::MIN),
//     };

//     for data in object_transform_map.values() {
//         for transform in &data.transforms {
//             let mut transformed_bbox = data.bbox.clone();
//             transformed_bbox.transform(transform);
//             if total_bbox.min.x == f32::MIN {
//                 total_bbox = transformed_bbox;
//             } else {
//                 total_bbox.unite(&transformed_bbox);
//             }
//         }
//     }

//     //draw the total bbox
//     add_bounding_box_wireframe(renderer, &total_bbox);
//     // println!("Bounding Box points are: {:?}", total_bbox);
//     // println!("Bounding Box size is: {:?}", total_bbox.delta());
//     unzoom_bbox(camera, &total_bbox);

//     total_bbox
// }

// fn process_object_id(
//     renderer: &mut renderer::Renderer,
//     package: &ThreemfPackage,
//     object_transform_map: &mut HashMap<usize, Data>,
//     gpu_mesh_map: &mut HashMap<usize, u32>,
//     object_id: usize,
//     transform: &Transformation,
// ) {
//     if let Entry::Vacant(e) = object_transform_map.entry(object_id) {
//         //process the mesh and create a GPU mesh
//         let object = package
//             .root
//             .resources
//             .object
//             .iter()
//             .find(|o| o.id == object_id);

//         if let Some(object) = object {
//             if let Some(mesh) = &object.mesh {
//                 create_gpu_mesh(renderer, gpu_mesh_map, object_id, transform, mesh, e);
//             } else if let Some(components) = &object.components {
//                 for component in &components.component {
//                     println!("There components: {:?}", component);
//                     // process_object_id(
//                     //     renderer,
//                     //     package,
//                     //     object_transform_map,
//                     //     gpu_mesh_map,
//                     //     object_id,
//                     //     transform,
//                     // );
//                 }
//             }
//         }
//     } else {
//         // add an additional instance transform
//         object_transform_map
//             .get_mut(&object_id)
//             .unwrap()
//             .transforms
//             .push(*transform);
//     }
// }

// fn create_gpu_mesh(
//     renderer: &mut renderer::Renderer,
//     gpu_mesh_map: &mut HashMap<usize, u32>,
//     mesh_id: usize,
//     transform: &Transformation,
//     mesh: &amrust_3mf::core::Mesh,
//     e: std::collections::hash_map::VacantEntry<'_, usize, Data>,
// ) {
//     let positions = convert_vertices_to_position(&mesh.vertices);
//     // println!("Number of vertices: {}", positions.len());
//     let indices = convert_triangles_to_indices(&mesh.triangles);
//     // println!("Number of triangles: {}", indices.len() / 3);
//     let color = convert_vertices_to_color(&mesh.vertices);
//     let wireframe_indices = convert_triangles_to_wireframe_indices(&mesh.triangles);

//     let gpu_mesh = MeshBuilder::new()
//         .add_vertex_stream(positions.as_slice())
//         .add_vertex_stream(color.as_slice())
//         .add_mesh_index_stream(indices.as_slice())
//         .add_wireframe_index_stream(wireframe_indices.as_slice())
//         .build(&renderer.device);

//     let gpu_mesh_id = renderer.add_mesh(gpu_mesh);

//     gpu_mesh_map.insert(mesh_id, gpu_mesh_id);

//     //calculate bounding box
//     let bbox = calculate_bounding_box(&mesh.vertices);

//     e.insert(Data {
//         bbox,
//         transforms: vec![transform.clone()],
//     });
// }

// pub fn unzoom_bbox(camera: &mut impl CameraData, total_bbox: &BoundingBox) {
//     let top_left_corner = Vec3::new(total_bbox.min.x, total_bbox.min.y, total_bbox.max.z);
//     // println!("The top left corner is: {}", top_left_corner);
//     camera
//         .transform(amrust_render::camera::CameraTransform::SetView {
//             eye_position: top_left_corner,
//             target_position: total_bbox.center(),
//             up_vector: Vec3::Z,
//         })
//         .transform(amrust_render::camera::CameraTransform::FitToExtent {
//             min: total_bbox.min,
//             max: total_bbox.max,
//         });
// }

// fn calculate_bounding_box(vertices: &Vertices) -> BoundingBox {
//     let mut min = glam::Vec3::splat(f32::INFINITY);
//     let mut max = glam::Vec3::splat(f32::NEG_INFINITY);

//     for vertex in &vertices.vertex {
//         let pos = glam::vec3(vertex.x as f32, vertex.y as f32, vertex.z as f32);
//         min = min.min(pos);
//         max = max.max(pos);
//     }

//     BoundingBox { min, max }
// }

// fn convert_vertices_to_position(vertices: &Vertices) -> Vec<Position> {
//     vertices
//         .vertex
//         .iter()
//         .map(|v| Position([v.x as f32, v.y as f32, v.z as f32]))
//         .collect()
// }

// fn convert_triangles_to_indices(triangles: &Triangles) -> Vec<u32> {
//     triangles
//         .triangle
//         .iter()
//         .flat_map(|t| vec![t.v1 as u32, t.v2 as u32, t.v3 as u32])
//         .collect()
// }

// fn convert_triangles_to_wireframe_indices(triangles: &Triangles) -> Vec<u32> {
//     let mut indices = Vec::new();
//     for t in &triangles.triangle {
//         indices.push(t.v1 as u32);
//         indices.push(t.v2 as u32);
//         indices.push(t.v2 as u32);
//         indices.push(t.v3 as u32);
//         indices.push(t.v3 as u32);
//         indices.push(t.v1 as u32);
//     }

//     indices
// }

// /// Returns a Vec of Vecs, where each inner Vec is a sorted island of triangle indices.
// fn sort_triangles_by_islands(triangles: &Triangles) -> Vec<Vec<usize>> {
//     // Map vertex index -> set of triangle indices that use it
//     let mut vertex_to_triangles: HashMap<usize, Vec<usize>> = HashMap::new();
//     for (i, tri) in triangles.triangle.iter().enumerate() {
//         assert!(
//             !is_triangle_degenerate(tri.v1, tri.v2, tri.v3),
//             "Degenerate triangle detected: v1={}, v2={}, v3={}",
//             tri.v1,
//             tri.v2,
//             tri.v3
//         );
//         for &v in &[tri.v1, tri.v2, tri.v3] {
//             vertex_to_triangles.entry(v).or_default().push(i);
//         }
//     }

//     let mut visited = vec![false; triangles.triangle.len()];
//     let mut islands = Vec::new();

//     for start in 0..triangles.triangle.len() {
//         if visited[start] {
//             continue;
//         }
//         let mut island = Vec::new();
//         let mut queue = VecDeque::new();
//         queue.push_back(start);
//         visited[start] = true;

//         while let Some(idx) = queue.pop_front() {
//             island.push(idx);
//             let tri = &triangles.triangle[idx];
//             for &v in &[tri.v1, tri.v2, tri.v3] {
//                 for &neighbor in &vertex_to_triangles[&v] {
//                     if !visited[neighbor] {
//                         visited[neighbor] = true;
//                         queue.push_back(neighbor);
//                     }
//                 }
//             }
//         }
//         islands.push(island);
//     }

//     println!("Number of islands are:{:?}", islands.len());
//     islands
// }

// // Example usage: flatten islands in order for rendering
// fn convert_triangles_to_indices_sorted(triangles: &Triangles) -> Vec<u32> {
//     let islands = sort_triangles_by_islands(triangles);
//     let mut indices = Vec::with_capacity(triangles.triangle.len() * 3);
//     for island in islands {
//         for &i in &island {
//             let t = &triangles.triangle[i];
//             indices.extend_from_slice(&[t.v1 as u32, t.v2 as u32, t.v3 as u32]);
//         }
//     }
//     indices
// }

// /// Checks if a triangle is degenerate.
// fn is_triangle_degenerate(v1: usize, v2: usize, v3: usize) -> bool {
//     // Check if any two vertices are the same
//     v1 == v2 || v2 == v3 || v3 == v1
// }

// fn convert_vertices_to_color(vertices: &Vertices) -> Vec<amrust_render::vertex::Color> {
//     vertices
//         .vertex
//         .iter()
//         .map(|_| amrust_render::vertex::Color([0.5, 0.5, 0.5]))
//         .collect()
// }

// fn convert_transform_to_glam_matrix(transform: &Transform) -> glam::Mat4 {
//     glam::Mat4::from_cols_array_2d(&[
//         [
//             transform.0[0] as f32,
//             transform.0[1] as f32,
//             transform.0[2] as f32,
//             0.0,
//         ],
//         [
//             transform.0[3] as f32,
//             transform.0[4] as f32,
//             transform.0[5] as f32,
//             0.0,
//         ],
//         [
//             transform.0[6] as f32,
//             transform.0[7] as f32,
//             transform.0[8] as f32,
//             0.0,
//         ],
//         [
//             transform.0[9] as f32,
//             transform.0[10] as f32,
//             transform.0[11] as f32,
//             1.0,
//         ],
//     ])
// }

// fn add_bounding_box_wireframe(
//     renderer: &mut amrust_render::renderer::Renderer,
//     bbox: &BoundingBox,
// ) -> u32 {
//     let mesh = MeshBuilder::new()
//         .add_vertex_stream(convert_points_vec_to_position(&bbox.corners()).as_slice())
//         .add_wireframe_index_stream(&BoundingBox::wireframe_indices())
//         .build(&renderer.device);

//     let mesh_id = renderer.add_mesh(mesh);

//     let wireframe_object = RenderObject {
//         renderable: amrust_render::Renderable::WireframeMesh(mesh_id),
//         instance: InstanceDataBuilder::new()
//             .add_instance_stream(&[Transformation(Mat4::IDENTITY).to_data()])
//             .add_instance_stream(&[Material::new(1.0, 1.0, 1.0).to_data()])
//             .build(&renderer.device),
//     };

//     renderer.add_object(wireframe_object)
// }

// fn convert_points_vec_to_position(points: &[Vec3]) -> Vec<Position> {
//     points.iter().map(|p| Position([p.x, p.y, p.z])).collect()
// }
