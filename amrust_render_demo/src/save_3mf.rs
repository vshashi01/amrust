use std::collections::HashMap;
use std::path::PathBuf;

use amrust_render::transformation::Transformation;
use async_trait::async_trait;
use thiserror::Error;
use threemf2::core::model::Unit;
use threemf2::core::transform::Transform;
use threemf2::io::ModelBuilder;
use threemf2::io::ObjectId;
use threemf2::io::ThreemfPackage;

use crate::amrust_db;
use crate::amrust_db::DbError;
use crate::amrust_db::Identifiable;
use crate::amrust_db::PartId;
use crate::amrust_db::PartInstance;
use crate::amrust_db::PartRep;
use crate::app_mode::AppMode;
use crate::operation::DbReader;
use crate::operation::Operation;
use crate::operation::OperationContext;
use crate::operation::OperationRequirements;
use crate::operation::OperationResponse;

#[derive(Debug, Error)]
pub enum DbTo3mfError {
    #[error("Unique part not found: {0}")]
    UniquePartNotFound(PartId),

    #[error("Error with database")]
    DbError(#[from] DbError),

    #[error("Somethign wrong with threemf process")]
    ThreemfProcessingError(#[from] threemf2::io::Error),

    #[error("Something wrong with building a 3MF Model")]
    ThreemfModelError(#[from] threemf2::io::ModelError),

    #[error("Something wrong with building a 3MF Mesh")]
    ThreemfMeshObjectError(#[from] threemf2::io::MeshObjectError),

    #[error("Something wrong with building a 3MF Composed Part")]
    ThreemfComponentsObjectError(#[from] threemf2::io::ComponentsObjectError),

    #[error("Not all components are processed already")]
    ComposedPartCannotBeProcessed,

    #[error("Scene is empty")]
    SceneEmpty,
}

pub struct Save3mfOps {
    pub path: PathBuf,
    pub app_mode: AppMode,
    pub entities_to_save: Vec<Identifiable>,
}

#[async_trait]
impl Operation for Save3mfOps {
    fn get_operation_requirements(&self) -> Option<OperationRequirements> {
        if self.entities_to_save.is_empty() {
            Some(OperationRequirements::ReadFullDb)
        } else {
            Some(OperationRequirements::Identifiables(
                self.entities_to_save.clone(),
            ))
        }
    }

    async fn execute(&mut self, context: &mut OperationContext) -> OperationResponse {
        let file = std::fs::File::create_new(&self.path);
        match file {
            Ok(f) => context
                .get_db(|db| match save(db, f) {
                    Ok(_) => OperationResponse::Succeeded("Save 3MF"),
                    Err(err) => OperationResponse::Failed("Save 3MF", Box::new(err)),
                })
                .await
                .unwrap(),
            Err(err) => OperationResponse::Failed("Save 3MF", Box::new(err)),
        }
    }
}

fn save(db: &dyn DbReader, threemf: std::fs::File) -> Result<(), DbTo3mfError> {
    let package = create_3mf_package(db)?;

    Ok(package.write(threemf)?)
}

// works best when parts are ordered such the they are return tip towards the root.
// single body parts first, then the composed of said single body parts
// then composed of other composed parts
// fn create_objects_map(parts: &[(PartId, &Part)]) -> Result<ModelBuilder, DbTo3mfError> {
//     let mut model_builder = ModelBuilder::new(Unit::Millimeter, true);
//     let mut unique_part_to_object_map: HashMap<PartId, ObjectId> = HashMap::new();
//     // let mut already_processed_composed_part = vec![];
//     for (part_id, part) in parts {
//         match part.get_rep() {
//             PartRep::Mesh(mesh) => {
//                 let object_id = process_and_insert_mesh_object(&mut model_builder, mesh)?;
//                 unique_part_to_object_map.insert(*part_id, object_id);
//             }
//             PartRep::ComposedPart(components) => {
//                 let can_process = components
//                     .iter()
//                     .all(|c| unique_part_to_object_map.contains_key(&c.id));

//                 if !can_process {
//                     continue;
//                 }

//                 let object_id = model_builder.add_components_object(|cb| {
//                     for c in components {
//                         if let Some(id) = unique_part_to_object_map.get(&c.id) {
//                             let transform = convert_transformation_to_3mf_transform(&c.transform);
//                             cb.add_component_advanced(*id, |c| {
//                                 c.transform(transform);
//                             });
//                         }
//                     }
//                     Ok(())
//                 })?;

//                 unique_part_to_object_map.insert(part.id.clone(), object_id);
//             }
//         }
//     }

//     Ok(model_builder)
// }

fn create_3mf_package(db: &dyn DbReader) -> Result<ThreemfPackage, DbTo3mfError> {
    let scene = db.get_scene().unwrap();
    // if scene.is_none() {
    //     return Err(DbTo3mfError::SceneEmpty);
    // }

    let mut model_builder = ModelBuilder::new(Unit::Millimeter, true);
    let mut unique_part_to_object_map: HashMap<PartId, ObjectId> = HashMap::new();
    let mut already_processed_composed_part = vec![];

    //try to process all unique parts.
    for (part_id, part) in db.get_parts() {
        match part.get_rep() {
            PartRep::Mesh(mesh) => {
                let object_id = process_and_insert_mesh_object(&mut model_builder, mesh)?;
                unique_part_to_object_map.insert(part_id, object_id);
            }
            PartRep::ComposedPart(instances) => {
                let mut instances_data = vec![];
                for i in instances {
                    let instance_data = db.get_part_instance(i).unwrap();
                    instances_data.push(instance_data);
                }
                //we only care about the processed objects if did not process then it proceeds
                if let Ok(object_id) = process_composed_part_and_insert_component_object(
                    &mut model_builder,
                    instances_data.as_slice(),
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
        .get_parts()
        .filter(|(_, part)| matches!(part.get_rep(), PartRep::ComposedPart(_)))
        .collect::<Vec<_>>();

    loop {
        if already_processed_composed_part.len() == all_composed_parts.len() {
            break;
        }

        for (part_id, part) in &all_composed_parts {
            if already_processed_composed_part.contains(part_id) {
                continue;
            }

            //only cares about the processed entity
            if let PartRep::ComposedPart(instances) = part.get_rep() {
                let mut instances_data = vec![];
                for i in instances {
                    let instance_data = db.get_part_instance(i).unwrap();
                    instances_data.push(instance_data);
                }
                if let Ok(object_id) = process_composed_part_and_insert_component_object(
                    &mut model_builder,
                    &instances_data,
                    &unique_part_to_object_map,
                ) {
                    already_processed_composed_part.push(*part_id);
                    unique_part_to_object_map.insert(*part_id, object_id);
                }
            }
        }
    }

    model_builder.add_build(None)?;
    for part_instance in &scene.instances {
        let instance_data = db.get_part_instance(part_instance).unwrap();
        match unique_part_to_object_map.get(&instance_data.part_id) {
            Some(object_id) => {
                let transform = convert_transformation_to_3mf_transform(&instance_data.transform);
                model_builder.add_build_item_advanced(*object_id, |i| {
                    i.transform(transform);
                })?;
            }
            None => {
                return Err(DbTo3mfError::UniquePartNotFound(instance_data.part_id));
            }
        }
    }

    let model = model_builder.build()?;

    Ok(model.into())
}

// fn create_3mf_package(db: &Db) -> Result<ThreemfPackage, DbTo3mfError> {
//     let scene = db.get_scene()?;
//     if db.is_scene_empty() {
//         return Err(DbTo3mfError::SceneEmpty);
//     }

//     let mut model_builder = ModelBuilder::new(Unit::Millimeter, true);
//     let mut unique_part_to_object_map: HashMap<PartId, ObjectId> = HashMap::new();
//     let mut already_processed_composed_part = vec![];

//     //try to process all unique parts.
//     for (part_id, part) in db.get_parts() {
//         match part.get_rep() {
//             PartRep::Mesh(mesh) => {
//                 let object_id = process_and_insert_mesh_object(&mut model_builder, mesh)?;
//                 unique_part_to_object_map.insert(part_id, object_id);
//             }
//             PartRep::ComposedPart(instances) => {
//                 let mut instances_data = vec![];
//                 for i in instances {
//                     let instance_data = db.get_part_instance_data(i)?;
//                     instances_data.push(instance_data);
//                 }
//                 //we only care about the processed objects if did not process then it proceeds
//                 if let Ok(object_id) = process_composed_part_and_insert_component_object(
//                     &mut model_builder,
//                     &instances_data,
//                     &unique_part_to_object_map,
//                 ) {
//                     already_processed_composed_part.push(part_id);
//                     unique_part_to_object_map.insert(part_id, object_id);
//                 }
//             }
//         }
//     }

//     //process remaining composed parts
//     let all_composed_parts = db
//         .get_parts()
//         .filter(|(_, part)| matches!(part.get_rep(), PartRep::ComposedPart(_)))
//         .collect::<Vec<_>>();

//     loop {
//         if already_processed_composed_part.len() == all_composed_parts.len() {
//             break;
//         }

//         for (part_id, part) in &all_composed_parts {
//             if already_processed_composed_part.contains(part_id) {
//                 continue;
//             }

//             //only cares about the processed entity
//             if let PartRep::ComposedPart(instances) = part.get_rep() {
//                 let mut instances_data = vec![];
//                 for i in instances {
//                     let instance_data = db.get_part_instance_data(i)?;
//                     instances_data.push(instance_data);
//                 }
//                 if let Ok(object_id) = process_composed_part_and_insert_component_object(
//                     &mut model_builder,
//                     &instances_data,
//                     &unique_part_to_object_map,
//                 ) {
//                     already_processed_composed_part.push(*part_id);
//                     unique_part_to_object_map.insert(*part_id, object_id);
//                 }
//             }
//         }
//     }

//     model_builder.add_build(None)?;
//     for part_instance in &scene.instances {
//         let instance_data = db.get_part_instance_data(part_instance)?;
//         match unique_part_to_object_map.get(&instance_data.part_id) {
//             Some(object_id) => {
//                 let transform = convert_transformation_to_3mf_transform(&instance_data.transform);
//                 model_builder.add_build_item_advanced(*object_id, |i| {
//                     i.transform(transform);
//                 })?;
//             }
//             None => {
//                 return Err(DbTo3mfError::UniquePartNotFound(instance_data.part_id));
//             }
//         }
//     }

//     let model = model_builder.build()?;

//     Ok(model.into())
// }

fn process_composed_part_and_insert_component_object(
    model_builder: &mut ModelBuilder,
    instances: &[&PartInstance],
    unique_part_to_object_map: &HashMap<PartId, ObjectId>,
) -> Result<ObjectId, DbTo3mfError> {
    let can_process = instances
        .iter()
        .all(|i| unique_part_to_object_map.contains_key(&i.part_id));

    if !can_process {
        return Err(DbTo3mfError::ComposedPartCannotBeProcessed);
    }

    let object_id = model_builder.add_components_object(|cb| {
        for instance in instances {
            if let Some(id) = unique_part_to_object_map.get(&instance.part_id) {
                let transform = convert_transformation_to_3mf_transform(&instance.transform);
                cb.add_component_advanced(*id, |c| {
                    c.transform(transform);
                });
            }
        }
        Ok(())
    })?;

    Ok(object_id)
}

fn process_and_insert_mesh_object(
    model_builder: &mut ModelBuilder,
    mesh: &amrust_db::Mesh,
) -> Result<ObjectId, DbTo3mfError> {
    match model_builder.add_mesh_object(|m| {
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

        Ok(())
    }) {
        Ok(id) => Ok(id),
        Err(err) => Err(DbTo3mfError::ThreemfMeshObjectError(err)),
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
