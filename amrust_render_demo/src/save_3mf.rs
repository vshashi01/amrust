use std::collections::HashMap;

use amrust_3mf::core::build::Build;
use amrust_3mf::core::build::Item;
use amrust_3mf::core::component::Component;
use amrust_3mf::core::component::Components;
use amrust_3mf::core::mesh::Mesh as Mesh3mf;
use amrust_3mf::core::mesh::Triangle;
use amrust_3mf::core::mesh::Triangles;
use amrust_3mf::core::mesh::Vertex;
use amrust_3mf::core::mesh::Vertices;
use amrust_3mf::core::model::Model;
use amrust_3mf::core::model::Unit;
use amrust_3mf::core::object::Object;
use amrust_3mf::core::object::ObjectType;
use amrust_3mf::core::resources::Resources;
use amrust_3mf::core::transform::Transform;
use amrust_3mf::io::ThreemfPackage;
use amrust_render::transformation::Transformation;
use glam::Vec3;
use thiserror::Error;

use crate::amrust_db;
use crate::amrust_db::Db;
use crate::amrust_db::DbError;
use crate::amrust_db::PartInstance;
use crate::amrust_db::PartRep;
use crate::amrust_db::UniquePartId;

#[derive(Debug, Error)]
pub enum DbTo3mfError {
    #[error("Unique part not found: {0}")]
    UniquePartNotFound(UniquePartId),

    #[error("Error with database")]
    DbError(#[from] DbError),

    #[error("Somethign wrong with threemf process")]
    ThreemfProcessingError(#[from] amrust_3mf::io::Error),

    #[error("Scene is empty")]
    SceneEmpty,
}

pub fn save(db: &Db, threemf: std::fs::File) -> Result<(), DbTo3mfError> {
    let package = create_3mf_package(db)?;

    Ok(package.write(threemf)?)
}

fn create_3mf_package(db: &Db) -> Result<ThreemfPackage, DbTo3mfError> {
    let scene = db.get_scene()?;

    if db.is_scene_empty() {
        return Err(DbTo3mfError::SceneEmpty);
    }

    let mut model = Model {
        unit: Some(Unit::Millimeter),
        requiredextensions: None,
        recommendedextensions: None,
        metadata: vec![],
        resources: Resources {
            object: vec![],
            basematerials: vec![],
        },
        build: Build {
            uuid: None,
            item: vec![],
        },
    };

    let mut object_index = 1;
    let mut unique_part_to_object_map: HashMap<UniquePartId, usize> = HashMap::new();

    let mut already_processed_composed_part = vec![];

    //try to process all unique parts.
    for (part_id, part_rep) in db.get_unique_parts() {
        match part_rep {
            PartRep::Mesh(mesh) => {
                match process_and_insert_mesh_object(&mut model, mesh, object_index) {
                    Ok(processed) => {
                        if processed {
                            unique_part_to_object_map.insert(part_id, object_index);
                            object_index += 1;
                        }
                    }
                    Err(err) => return Err(err),
                }
            }
            PartRep::ComposedPart(instances) => {
                match process_composed_part_and_insert_component_object(
                    &mut model,
                    instances,
                    &unique_part_to_object_map,
                    object_index,
                ) {
                    Ok(processed) => {
                        if processed {
                            already_processed_composed_part.push(part_id);
                            unique_part_to_object_map.insert(part_id, object_index);
                            object_index += 1;
                        }
                    }
                    Err(err) => return Err(err),
                }
            }
        }
    }

    //process remaining composed parts
    let all_composed_parts = db
        .get_unique_parts()
        .filter(|(_, part_rep)| matches!(part_rep, PartRep::ComposedPart(_)))
        .collect::<Vec<_>>();

    loop {
        if already_processed_composed_part.len() == all_composed_parts.len() {
            break;
        }

        for (part_id, composed_rep) in &all_composed_parts {
            if already_processed_composed_part.contains(part_id) {
                continue;
            }

            if let PartRep::ComposedPart(instances) = composed_rep {
                match process_composed_part_and_insert_component_object(
                    &mut model,
                    instances,
                    &unique_part_to_object_map,
                    object_index,
                ) {
                    Ok(processed) => {
                        if processed {
                            already_processed_composed_part.push(*part_id);
                            unique_part_to_object_map.insert(*part_id, object_index);
                            object_index += 1;
                        }
                    }
                    Err(err) => return Err(err),
                }
            }
        }
    }

    for part_instance in &scene.instances {
        match unique_part_to_object_map.get(&part_instance.part_id) {
            Some(object_id) => {
                process_and_add_build_item(&mut model, object_id, &part_instance.transform)
            }
            None => {
                return Err(DbTo3mfError::UniquePartNotFound(part_instance.part_id));
            }
        }
    }

    Ok(model.into())
}

fn process_and_add_build_item(model: &mut Model, object_id: &usize, transform: &Transformation) {
    model.build.item.push(Item {
        objectid: *object_id,
        transform: Some(convert_transformation_to_3mf_transform(transform)),
        partnumber: None,
        path: None,
        uuid: None,
    });
}

fn process_composed_part_and_insert_component_object(
    model: &mut Model,
    instances: &[PartInstance],
    unique_part_to_object_map: &HashMap<UniquePartId, usize>,
    object_index: usize,
) -> Result<bool, DbTo3mfError> {
    let can_process = instances
        .iter()
        .all(|i| unique_part_to_object_map.contains_key(&i.part_id));

    if !can_process {
        return Ok(false);
    }

    let mut components = vec![];
    for i in instances {
        if let Some(object_id) = unique_part_to_object_map.get(&i.part_id) {
            let component = Component {
                objectid: *object_id,
                transform: Some(convert_transformation_to_3mf_transform(&i.transform)),
                path: None,
                uuid: None,
            };

            components.push(component);
        }
    }

    if components.len() == instances.len() {
        model.resources.object.push(Object {
            id: object_index,
            objecttype: Some(ObjectType::Model),
            thumbnail: None,
            partnumber: None,
            name: None,
            pid: None,
            pindex: None,
            uuid: None,
            mesh: None,
            components: Some(Components {
                component: components,
            }),
        });

        return Ok(true);
    }

    Ok(false)
}

fn process_and_insert_mesh_object(
    model: &mut Model,
    mesh: &amrust_db::Mesh,
    object_index: usize,
) -> Result<bool, DbTo3mfError> {
    let vertices = get_3mf_vertices(&mesh.vertices);
    let triangles = get_3mf_triangles(&mesh.triangles);
    let mesh_3mf = get_3mf_mesh(vertices, triangles);
    model.resources.object.push(Object {
        id: object_index,
        objecttype: Some(amrust_3mf::core::object::ObjectType::Model),
        thumbnail: None,
        partnumber: None,
        name: None,
        pid: None,
        pindex: None,
        uuid: None,
        mesh: Some(mesh_3mf),
        components: None,
    });

    Ok(true)
}

fn get_3mf_vertices(vertices: &[Vec3]) -> Vertices {
    let vertices = vertices
        .iter()
        .map(|v| Vertex {
            x: v.x as f64,
            y: v.y as f64,
            z: v.z as f64,
        })
        .collect::<Vec<_>>();

    Vertices { vertex: vertices }
}

fn get_3mf_triangles(triangles: &[u32]) -> Triangles {
    let triangles = triangles
        .chunks_exact(3)
        .map(|t| Triangle {
            v1: t[0] as usize,
            v2: t[1] as usize,
            v3: t[2] as usize,
            p1: None,
            p2: None,
            p3: None,
            pid: None,
        })
        .collect::<Vec<_>>();

    Triangles {
        triangle: triangles,
    }
}

fn get_3mf_mesh(vertices: Vertices, triangles: Triangles) -> amrust_3mf::core::mesh::Mesh {
    Mesh3mf {
        vertices,
        triangles,
        trianglesets: None,
        beamlattice: None,
    }
}

fn convert_transformation_to_3mf_transform(transform: &Transformation) -> Transform {
    let cols = transform.0.to_cols_array_2d();

    Transform([
        cols[0][0] as f64,
        cols[0][1] as f64,
        cols[0][2] as f64,
        cols[1][0] as f64,
        cols[1][1] as f64,
        cols[1][2] as f64,
        cols[2][0] as f64,
        cols[2][1] as f64,
        cols[2][2] as f64,
        cols[3][0] as f64,
        cols[3][1] as f64,
        cols[3][2] as f64,
    ])
}
