use std::collections::HashMap;

use amrust_3mf::core::model::Unit;
use amrust_3mf::core::transform::Transform;
use amrust_3mf::io::ModelBuilder;
use amrust_3mf::io::ObjectId;
use amrust_3mf::io::ThreemfPackage;
use amrust_render::transformation::Transformation;
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

    #[error("Something wrong with building a 3MF Model")]
    ThreemfBuilderError(#[from] amrust_3mf::io::BuilderError),

    #[error("Not all components are processed already")]
    ComposedPartCannotBeProcessed,

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

    let mut model_builder = ModelBuilder::new(Unit::Millimeter);
    let mut unique_part_to_object_map: HashMap<UniquePartId, ObjectId> = HashMap::new();
    let mut already_processed_composed_part = vec![];

    //try to process all unique parts.
    for (part_id, part_rep) in db.get_unique_parts() {
        match part_rep {
            PartRep::Mesh(mesh) => {
                let object_id = process_and_insert_mesh_object(&mut model_builder, mesh)?;
                unique_part_to_object_map.insert(part_id, object_id);
            }
            PartRep::ComposedPart(instances) => {
                //we only care about the processed objects if did not process then it proceeds
                if let Ok(object_id) = process_composed_part_and_insert_component_object(
                    &mut model_builder,
                    instances,
                    &unique_part_to_object_map,
                ) {
                    already_processed_composed_part.push(part_id);
                    unique_part_to_object_map.insert(part_id, object_id);
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

            //only cares about the processed entity
            if let PartRep::ComposedPart(instances) = composed_rep
                && let Ok(object_id) = process_composed_part_and_insert_component_object(
                    &mut model_builder,
                    instances,
                    &unique_part_to_object_map,
                )
            {
                already_processed_composed_part.push(*part_id);
                unique_part_to_object_map.insert(*part_id, object_id);
            }
        }
    }

    for part_instance in &scene.instances {
        match unique_part_to_object_map.get(&part_instance.part_id) {
            Some(object_id) => {
                let transform = convert_transformation_to_3mf_transform(&part_instance.transform);
                model_builder.add_build_item_advanced(*object_id, Some(transform), None);
            }
            None => {
                return Err(DbTo3mfError::UniquePartNotFound(part_instance.part_id));
            }
        }
    }

    let model = model_builder.build();

    Ok(model.into())
}

fn process_composed_part_and_insert_component_object(
    model_builder: &mut ModelBuilder,
    instances: &[PartInstance],
    unique_part_to_object_map: &HashMap<UniquePartId, ObjectId>,
) -> Result<ObjectId, DbTo3mfError> {
    let can_process = instances
        .iter()
        .all(|i| unique_part_to_object_map.contains_key(&i.part_id));

    if !can_process {
        return Err(DbTo3mfError::ComposedPartCannotBeProcessed);
    }

    let object_id = model_builder.add_object(|o| {
        let _ = o.composed_part(|cb| {
            for instance in instances {
                if let Some(id) = unique_part_to_object_map.get(&instance.part_id) {
                    let transform = convert_transformation_to_3mf_transform(&instance.transform);
                    cb.add_component(*id, Some(transform));
                }
            }

            Ok(cb)
        });
    })?;

    Ok(object_id)
}

fn process_and_insert_mesh_object(
    model_builder: &mut ModelBuilder,
    mesh: &amrust_db::Mesh,
) -> Result<ObjectId, DbTo3mfError> {
    match model_builder.add_object(|o| {
        let _ = o.mesh(|m| {
            m.add_vertices(
                mesh.vertices
                    .iter()
                    .map(|v| [v.x as f64, v.y as f64, v.z as f64])
                    .collect::<Vec<_>>()
                    .as_ref(),
            )
            .add_triangles_flat(
                mesh.triangles
                    .iter()
                    .map(|t| *t as usize)
                    .collect::<Vec<_>>()
                    .as_ref(),
            );
        });
    }) {
        Ok(id) => Ok(id),
        Err(err) => Err(DbTo3mfError::ThreemfBuilderError(err)),
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
