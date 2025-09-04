use amrust_3mf::query::get_object_ref_from_id;
use glam::Vec3;
use thiserror::Error;

use amrust_3mf::core::transform::Transform;
use amrust_3mf::core::{Triangle, Vertex};
use amrust_3mf::io::ThreemfPackage;
use amrust_render::transformation::Transformation;

use crate::amrust_db::{Db, DbError, Mesh, PartInstance, PartRep, Scene};

use core::f32;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Error)]
pub enum DbFrom3mfError {
    #[error("Object not found: {0}")]
    ObjectNotFound(usize),

    #[error("Error with database")]
    DbError(#[from] DbError),

    #[error("The Composed parts is empty {0}")]
    EmptyComposedPart(usize),

    #[error("Object does not contain data {0}")]
    EmptyObject(usize),
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
        let transform = get_transformation(&item.transform);
        items_transform_pair.push((object_id, item.path.clone(), transform));
    }

    //setup the Mesh, PartRep and Part based on the items on the scene
    //what to do with objects that are not listed in the build?
    for item in items_transform_pair {
        if let Some(ids) = part_rep_map.get_key_value(&item.0) {
            parts_in_scene.push(PartInstance {
                part_id: ids.1.1, //set the unique part id
                transform: item.2,
            });
        } else {
            let part_id = process_object_and_register_unique_part(
                &mut db,
                &mut part_rep_map,
                item.0,
                &package,
                &item.1,
                None,
            )?;

            parts_in_scene.push(PartInstance {
                part_id,
                transform: item.2,
            });
        }
    }

    db.add_scene(Scene(parts_in_scene));

    Ok(db)
}

fn process_object_and_register_unique_part(
    db: &mut Db,
    part_rep_map: &mut HashMap<usize, (usize, usize)>,
    object_id: usize,
    package: &ThreemfPackage,
    path: &Option<String>,
    parent_model: Option<String>,
) -> Result<usize, DbFrom3mfError> {
    let (object, parent_model_path) =
        get_object_ref_from_id(object_id, package, path, &parent_model);

    match object {
        Some(object) => {
            if let Some(m) = &object.mesh {
                process_mesh_object(db, part_rep_map, object.id, m)
            } else if let Some(comps) = &object.components {
                process_composed_object(
                    db,
                    part_rep_map,
                    object.id,
                    comps,
                    package,
                    parent_model_path,
                )
            } else {
                Err(DbFrom3mfError::EmptyObject(object_id))
            }
        }
        None => Err(DbFrom3mfError::ObjectNotFound(object_id)),
    }
}

fn process_mesh_object(
    db: &mut Db,
    part_rep_map: &mut HashMap<usize, (usize, usize)>,
    object_id: usize,
    m: &amrust_3mf::core::Mesh,
) -> Result<usize, DbFrom3mfError> {
    let mesh = Mesh {
        vertices: convert_3mf_vertices_to_mesh_vertices(&m.vertices.vertex),
        triangles: convert_3mf_triangles_to_mesh_triangles(&m.triangles.triangle),
    };

    let registered = db.add_part_rep(PartRep::Mesh(Box::new(mesh)))?;
    part_rep_map.insert(object_id, registered);
    Ok(registered.1)
}

fn process_composed_object(
    db: &mut Db,
    part_rep_map: &mut HashMap<usize, (usize, usize)>,
    object_id: usize,
    comps: &amrust_3mf::core::component::Components,
    package: &ThreemfPackage,
    parent_model: Option<String>,
) -> Result<usize, DbFrom3mfError> {
    let mut list_of_unique_part_id_per_component = vec![];
    for comp in &comps.component {
        let transform = get_transformation(&comp.transform);

        if let Some(partids) = part_rep_map.get(&comp.objectid) {
            list_of_unique_part_id_per_component.push(PartInstance {
                part_id: partids.1,
                transform,
            });
        } else {
            let part_id = process_object_and_register_unique_part(
                db,
                part_rep_map,
                comp.objectid,
                package,
                &comp.path,
                parent_model.clone(),
            )?;
            list_of_unique_part_id_per_component.push(PartInstance { part_id, transform });
        }
    }

    if !list_of_unique_part_id_per_component.is_empty() {
        let registered =
            db.add_part_rep(PartRep::ComposedPart(list_of_unique_part_id_per_component))?;
        part_rep_map.insert(object_id, registered);

        Ok(registered.1)
    } else {
        Err(DbFrom3mfError::EmptyComposedPart(object_id))
    }
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

fn get_transformation(
    transform: &Option<amrust_3mf::core::transform::Transform>,
) -> Transformation {
    {
        if let Some(transform) = transform {
            Transformation(convert_transform_to_glam_matrix(transform))
        } else {
            Transformation(glam::Mat4::IDENTITY)
        }
    }
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
