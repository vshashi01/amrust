use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use log::info;
use smol::Timer;
use thiserror::Error;
use threemf2::core::model::Unit;
use threemf2::core::transform::Transform;
use threemf2::io::ModelBuilder;
use threemf2::io::ObjectId;
use threemf2::io::ThreemfPackage;

use crate::core::amrust_db::DbError;
use crate::core::app_mode::AppMode;
use crate::core::interfaces::command::Command;
use crate::core::interfaces::command::CommandCategory;
use crate::core::interfaces::command::CommandContext;
use crate::core::interfaces::db_reader::DbReader;
use crate::core::interfaces::operation::Operation;
use crate::core::interfaces::operation::OperationNature;
use crate::core::interfaces::operation::OperationResponse;
use crate::core::services::command_service::CommandService;
use crate::core::services::operation_service::OperationServiceRequest;
use crate::core::types::db_context::DbContext;
use crate::core::types::identifiable::Identifiable;
use crate::core::types::mesh::Mesh;
use crate::core::types::part::Part;
use crate::core::types::part::PartId;
use crate::core::types::part::PartRep;
use crate::core::types::part_instance::PartInstance;
use crate::core::types::part_instance::PartInstanceId;
use crate::core::types::transformation::Transformation;

#[derive(Debug, Error)]
pub enum DbTo3mfError {
    // #[error("Unique part not found: {0}")]
    // UniquePartNotFound(PartId),
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

    // #[error("Not all components are processed already")]
    // ComposedPartCannotBeProcessed,
    #[error("Scene is empty")]
    SceneEmpty,

    #[error("No Parts to be saved")]
    NoPartsToBeSaved,
}

pub enum SaveMode {
    Scene,
    PartsOnly(Vec<PartId>),
    PartInstances(Vec<PartInstanceId>),
}

pub struct Save3mfOps {
    pub path: PathBuf,
    pub save_mode: SaveMode,
}

#[async_trait]
impl Operation for Save3mfOps {
    fn name(&self) -> &str {
        match &self.save_mode {
            SaveMode::Scene => "Saving whole Scene to 3MF",
            SaveMode::PartsOnly(_) => "Saving Parts to 3MF",
            SaveMode::PartInstances(_) => "Saving Part Instance to 3MF",
        }
    }

    fn get_operation_requirements(&self) -> Option<OperationNature> {
        let req = match &self.save_mode {
            SaveMode::Scene => OperationNature::ReadOnlyInModal,
            SaveMode::PartsOnly(part_ids) => OperationNature::ModifyExistingFromBackground {
                identifiables: part_ids.iter().map(|id| Identifiable::Part(*id)).collect(),
                detach_parts_with_part_instances: false,
            },
            SaveMode::PartInstances(part_instance_ids) => {
                OperationNature::ModifyExistingFromBackground {
                    identifiables: part_instance_ids
                        .iter()
                        .map(|id| Identifiable::PartInstance(*id))
                        .collect(),
                    detach_parts_with_part_instances: true,
                }
            }
        };

        Some(req)
        // Some(OperationRequirements::ReadFullDb)
    }

    async fn execute(&mut self, context: &mut DbContext) -> OperationResponse {
        let file = std::fs::File::create_new(&self.path);
        match file {
            Ok(f) => {
                match &self.save_mode {
                    SaveMode::PartInstances(_) | SaveMode::PartsOnly(_) => {
                        Timer::after(Duration::from_secs(15)).await;
                    }
                    SaveMode::Scene => {
                        Timer::after(Duration::from_secs(10)).await;
                    }
                }

                match context
                    .get_db(|db| match save(db, &self.save_mode, f) {
                        Ok(_) => OperationResponse::Succeeded { name: "Save 3MF" },
                        Err(err) => OperationResponse::Failed {
                            name: "Save 3MF",
                            error: Box::new(err),
                            is_restore_db_required: false,
                        },
                    })
                    .await
                {
                    Ok(resp) => resp,
                    Err(err) => OperationResponse::Failed {
                        name: "Save 3MF",
                        error: Box::new(err),
                        is_restore_db_required: false,
                    },
                }
            }
            Err(err) => OperationResponse::Failed {
                name: "Save 3MF",
                error: Box::new(err),
                is_restore_db_required: false,
            },
        }
    }
}

fn save(
    db: &dyn DbReader,
    save_mode: &SaveMode,
    threemf: std::fs::File,
) -> Result<(), DbTo3mfError> {
    let model_builder = match save_mode {
        SaveMode::Scene => {
            log::debug!("Saving Scene");
            save_scene(db)
        }
        SaveMode::PartsOnly(part_ids) => {
            if !part_ids.is_empty() {
                log::debug!("Saving Parts");
                save_parts(db, part_ids)
            } else {
                Err(DbTo3mfError::NoPartsToBeSaved)
            }
        }
        SaveMode::PartInstances(part_instance_ids) => {
            if !part_instance_ids.is_empty() {
                log::debug!("Saving Specific Instances");
                save_instances(db, part_instance_ids)
            } else {
                Err(DbTo3mfError::NoPartsToBeSaved)
            }
        }
    };

    match model_builder {
        Ok(builder) => {
            let model = builder.build()?;
            let package: ThreemfPackage = model.into();

            Ok(package.write(threemf)?)
        }
        Err(err) => {
            log::error!("Something went wrong: {err:?}");
            Err(err)
        }
    }
}

fn save_scene(db: &dyn DbReader) -> Result<ModelBuilder, DbTo3mfError> {
    let mut model_builder = ModelBuilder::new(Unit::Millimeter, true);
    model_builder.add_build(None)?;

    let part_instances_to_save = if let Some(scene) = db.get_scene() {
        if scene.instances.is_empty() {
            return Err(DbTo3mfError::SceneEmpty);
        } else {
            scene.instances.clone()
        }
    } else {
        return Err(DbTo3mfError::SceneEmpty);
    };

    let mut parts_already_processed =
        HashMap::<PartId, ObjectId>::with_capacity(part_instances_to_save.len());
    let mut parts_to_be_processed = Vec::<PartId>::with_capacity(part_instances_to_save.len());

    let instance_map: HashMap<PartInstanceId, PartInstance> = db
        .get_part_instances()
        .map(|(id, instance, _)| (id, instance.clone()))
        .collect();

    // this is needed to identify all the parts that needs to be saved.
    let parts_to_save = create_list_of_all_parts_to_save(db, &part_instances_to_save);

    add_parts_to_model_builder(
        db,
        &mut model_builder,
        &mut parts_already_processed,
        &mut parts_to_be_processed,
        &instance_map,
        &parts_to_save,
    )?;

    for id in part_instances_to_save {
        if let Some(instance) = instance_map.get(&id)
            && let Some(object_id) = parts_already_processed.get(&instance.part_id)
        {
            let transform = convert_transformation_to_3mf_transform(&instance.transform);
            model_builder.add_build_item_advanced(*object_id, |builder| {
                builder.transform(transform);
            })?;
        }
    }

    Ok(model_builder)
}

fn save_instances(
    db: &dyn DbReader,
    part_instances_to_save: &[PartInstanceId],
) -> Result<ModelBuilder, DbTo3mfError> {
    let mut model_builder = ModelBuilder::new(Unit::Millimeter, true);
    model_builder.add_build(None)?;

    //just guessing the capacity based on number of part_instances_to_save
    let mut parts_already_processed =
        HashMap::<PartId, ObjectId>::with_capacity(part_instances_to_save.len());
    let mut parts_to_be_processed = Vec::with_capacity(part_instances_to_save.len());

    let instance_map: HashMap<PartInstanceId, PartInstance> = db
        .get_part_instances()
        .map(|(id, instance, _)| (id, instance.clone()))
        .collect();

    // this is needed to identify all the parts that needs to be saved.
    let parts_to_save = create_list_of_all_parts_to_save(db, part_instances_to_save);

    add_parts_to_model_builder(
        db,
        &mut model_builder,
        &mut parts_already_processed,
        &mut parts_to_be_processed,
        &instance_map,
        &parts_to_save,
    )?;

    for id in part_instances_to_save {
        if let Some(instance) = instance_map.get(id)
            && let Some(object_id) = parts_already_processed.get(&instance.part_id)
        {
            let transform = convert_transformation_to_3mf_transform(&instance.transform);
            model_builder.add_build_item_advanced(*object_id, |builder| {
                builder.transform(transform);
            })?;
        }
    }

    Ok(model_builder)
}

fn save_parts(db: &dyn DbReader, parts_to_save: &[PartId]) -> Result<ModelBuilder, DbTo3mfError> {
    let mut model_builder = ModelBuilder::new(Unit::Millimeter, true);
    model_builder.add_build(None)?;

    //just guessing the capacity based on number of part_instances_to_save
    let mut parts_already_processed =
        HashMap::<PartId, ObjectId>::with_capacity(parts_to_save.len());
    let mut parts_to_be_processed = Vec::with_capacity(parts_to_save.len());

    let instance_map: HashMap<PartInstanceId, PartInstance> = db
        .get_part_instances()
        .map(|(id, instance, _)| (id, instance.clone()))
        .collect();

    add_parts_to_model_builder(
        db,
        &mut model_builder,
        &mut parts_already_processed,
        &mut parts_to_be_processed,
        &instance_map,
        parts_to_save,
    )?;

    // add a build item for every part
    for id in parts_to_save {
        if let Some(object_id) = parts_already_processed.get(id) {
            model_builder.add_build_item(*object_id)?;
        }
    }
    Ok(model_builder)
}

fn add_parts_to_model_builder(
    db: &dyn DbReader,
    model_builder: &mut ModelBuilder,
    parts_already_processed: &mut HashMap<PartId, ObjectId>,
    parts_to_be_processed: &mut Vec<PartId>,
    instance_map: &HashMap<PartInstanceId, PartInstance>,
    parts_to_save: &[PartId],
) -> Result<(), DbTo3mfError> {
    for (id, part) in db.get_parts() {
        if parts_to_save.contains(&id) {
            process_reps(
                model_builder,
                parts_already_processed,
                parts_to_be_processed,
                instance_map,
                &id,
                part,
            )?;
        }
    }

    process_all_remaining_parts(
        db,
        model_builder,
        parts_already_processed,
        parts_to_be_processed,
        instance_map,
    )?;

    Ok(())
}

fn process_all_remaining_parts(
    db: &(dyn DbReader + 'static),
    model_builder: &mut ModelBuilder,
    parts_already_processed: &mut HashMap<PartId, ObjectId>,
    parts_to_be_processed: &mut Vec<PartId>,
    instance_map: &HashMap<PartInstanceId, PartInstance>,
) -> Result<(), DbTo3mfError> {
    loop {
        if parts_to_be_processed.is_empty() {
            break;
        }

        for part_id in parts_to_be_processed.clone() {
            match db.get_part(&part_id) {
                Some(part) => match part.get_rep() {
                    PartRep::Mesh(mesh) => {
                        match process_and_insert_mesh_object(model_builder, mesh) {
                            Ok(object_id) => {
                                parts_already_processed.insert(part_id, object_id);

                                if let Some(pos) =
                                    parts_to_be_processed.iter().position(|id| id == &part_id)
                                {
                                    parts_to_be_processed.remove(pos);
                                }
                            }
                            Err(err) => return Err(err),
                        }
                    }
                    PartRep::ComposedPart(part_instance_ids) => {
                        let unprocessed_instances = get_unprocessed_components(
                            parts_already_processed,
                            instance_map,
                            part_instance_ids,
                        )
                        .collect::<Vec<_>>();

                        if unprocessed_instances.is_empty() {
                            let instances = part_instance_ids
                                .iter()
                                .filter_map(|id| instance_map.get(id))
                                .collect::<Vec<_>>();

                            match process_composed_part_and_insert_component_object(
                                model_builder,
                                &instances,
                                parts_already_processed,
                            ) {
                                Ok(object_id) => {
                                    parts_already_processed.insert(part_id, object_id);
                                    if let Some(pos) =
                                        parts_to_be_processed.iter().position(|id| id == &part_id)
                                    {
                                        parts_to_be_processed.remove(pos);
                                    }
                                }
                                Err(err) => return Err(err),
                            }
                        } else {
                            for instance in unprocessed_instances {
                                if !parts_to_be_processed.contains(&instance.part_id) {
                                    parts_to_be_processed.push(instance.part_id);
                                }
                            }
                        }
                    }
                },
                None => panic!("Unable to get some Part from Db"),
            }
        }
    }

    Ok(())
}

fn process_reps(
    model_builder: &mut ModelBuilder,
    parts_already_processed: &mut HashMap<PartId, ObjectId>,
    parts_to_be_processed: &mut Vec<PartId>,
    instance_map: &HashMap<PartInstanceId, PartInstance>,
    id: &PartId,
    part: &Part,
) -> Result<(), DbTo3mfError> {
    if parts_already_processed.contains_key(id) {
        return Ok(());
    }

    match part.get_rep() {
        PartRep::Mesh(mesh) => match process_and_insert_mesh_object(model_builder, mesh) {
            Ok(object_id) => {
                parts_already_processed.insert(*id, object_id);
            }
            Err(err) => return Err(err),
        },
        PartRep::ComposedPart(part_instance_ids) => {
            let unprocessed_instances = get_unprocessed_components(
                parts_already_processed,
                instance_map,
                part_instance_ids,
            )
            .collect::<Vec<_>>();
            if unprocessed_instances.is_empty() {
                let instances = part_instance_ids
                    .iter()
                    .filter_map(|id| instance_map.get(id))
                    .collect::<Vec<_>>();

                match process_composed_part_and_insert_component_object(
                    model_builder,
                    &instances,
                    parts_already_processed,
                ) {
                    Ok(object_id) => {
                        // if processing succeeds then remove from parts to be processed
                        parts_already_processed.insert(*id, object_id);
                    }
                    Err(err) => return Err(err),
                }
            } else {
                parts_to_be_processed.push(*id);
            }
        }
    }

    Ok(())
}

fn get_unprocessed_components<'a>(
    parts_already_processed: &HashMap<PartId, ObjectId>,
    instance_map: &'a HashMap<PartInstanceId, PartInstance>,
    part_instance_ids: &[PartInstanceId],
) -> impl Iterator<Item = &'a PartInstance> {
    instance_map.iter().filter_map(|(id, instance)| {
        if part_instance_ids.contains(id)
            && !parts_already_processed.contains_key(&instance.part_id)
        {
            Some(instance)
        } else {
            None
        }
    })
}

// Accepts the user facing instances as input and then creates a new list that includes
// all instances to be processed. Since Composed Parts are a collection of instances
// these instances should also be processed and saved.
fn create_list_of_all_parts_to_save(
    db: &dyn DbReader,
    instance_to_save: &[PartInstanceId],
) -> Vec<PartId> {
    let mut instances_to_visit = instance_to_save.to_vec();
    let mut all_parts_to_save = vec![];
    loop {
        if instances_to_visit.is_empty() {
            break;
        }

        for (id, instance_data, part) in db.get_part_instances() {
            if instances_to_visit.contains(&id) {
                //any instances that are visited anyways needs to be saved to get the full tree.
                if !all_parts_to_save.contains(&instance_data.part_id) {
                    all_parts_to_save.push(instance_data.part_id);
                }

                match part.get_rep() {
                    PartRep::Mesh(_) => {}
                    PartRep::ComposedPart(part_instance_ids) => {
                        for comp in part_instance_ids {
                            if !instances_to_visit.contains(comp) {
                                instances_to_visit.push(*comp);
                            }
                        }
                    }
                }

                // we have already visited this instances
                if let Some(pos) = instances_to_visit.iter().position(|i| *i == id) {
                    instances_to_visit.remove(pos);
                }
            }
        }
    }

    all_parts_to_save
}

fn process_composed_part_and_insert_component_object(
    model_builder: &mut ModelBuilder,
    instances: &[&PartInstance],
    unique_part_to_object_map: &HashMap<PartId, ObjectId>,
) -> Result<ObjectId, DbTo3mfError> {
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
    mesh: &Mesh,
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

pub struct SaveSceneCommand;

impl Command for SaveSceneCommand {
    fn id(&self) -> &str {
        "save_scene"
    }

    fn label(&self) -> &str {
        "Save Scene to 3mf"
    }

    fn is_enabled(&self, context: &CommandContext) -> bool {
        !context.db_view_model.is_database_empty()
    }

    fn category(&self) -> CommandCategory {
        CommandCategory::File
    }

    fn execute(&self, context: &mut CommandContext) {
        // Show file dialog with handler for importing 3MF files
        context.file_dialog_service_request_tx.show_save_dialog(
            "3D Manufacturing Format",
            "3mf",
            |path: PathBuf, ctx: &mut CommandContext| {
                if let Some(ext) = path.extension()
                    && ext == "3mf"
                {
                    let ops = Save3mfOps {
                        path,
                        save_mode: SaveMode::Scene,
                    };
                    if let Err(err) = ctx
                        .operation_queue_tx
                        .send_blocking(OperationServiceRequest::ModalOpWait(Box::new(ops)))
                    {
                        log::error!("Failed to queue import operation: {err:?}");
                    }
                }
            },
        );
    }
}

pub struct SavePartCommand;

impl Command for SavePartCommand {
    fn id(&self) -> &str {
        "save_parts_to_3mf"
    }

    fn label(&self) -> &str {
        "Save Selected Parts to 3mf"
    }

    fn is_enabled(&self, context: &CommandContext) -> bool {
        !context
            .db_view_model
            .get_all_operable_selected_identifiables()
            .is_empty()
    }

    fn category(&self) -> CommandCategory {
        CommandCategory::File
    }

    fn execute(&self, context: &mut CommandContext) {
        context.file_dialog_service_request_tx.show_save_dialog(
            "3D Manufacturing Format",
            "3mf",
            |path: PathBuf, ctx: &mut CommandContext| {
                if let Some(ext) = path.extension()
                    && ext == "3mf"
                {
                    let operable_selected_identifiables =
                        ctx.db_view_model.get_all_operable_selected_identifiables();
                    let ops_msg = {
                        if !operable_selected_identifiables.is_empty() {
                            let save_mode = match ctx.current_app_mode {
                                AppMode::Objects => {
                                    let parts =
                                        operable_selected_identifiables.iter().filter_map(|i| {
                                            match i {
                                                Identifiable::Part(part_id) => Some(*part_id),
                                                Identifiable::PartInstance(_) => None,
                                            }
                                        });

                                    SaveMode::PartsOnly(parts.collect())
                                }
                                AppMode::Build => {
                                    let part_instances = operable_selected_identifiables
                                        .iter()
                                        .filter_map(|i| match i {
                                            Identifiable::Part(_) => None,

                                            Identifiable::PartInstance(part_instance_id) => {
                                                Some(*part_instance_id)
                                            }
                                        });

                                    SaveMode::PartInstances(part_instances.collect())
                                }
                            };
                            OperationServiceRequest::BackgroundOp(Box::new(Save3mfOps {
                                path,
                                save_mode,
                            }))
                        } else {
                            OperationServiceRequest::ModalOp(Box::new(Save3mfOps {
                                path,
                                save_mode: SaveMode::Scene,
                            }))
                        }
                    };
                    if let Err(err) = ctx.operation_queue_tx.send_blocking(ops_msg) {
                        log::error!("Failed to queue save part operation: {err:?}");
                    }
                }
            },
        );
    }
}

/// Register all commands provided by the load_3mf module
pub fn register_commands(commands_service: &mut CommandService) {
    commands_service.register_command(Box::new(SaveSceneCommand));
    commands_service.register_command(Box::new(SavePartCommand));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs::File, path::PathBuf};

    use crate::{core::types::part::PartRep, features::load_3mf};

    #[test]
    fn test_save_scene() {
        let threemf_file = File::open(PathBuf::from("../mesh-composedpart.3mf")).unwrap();
        let db = load_3mf::load(threemf_file).unwrap();
        let builder = save_scene(&db).unwrap();
        let model = builder.build().unwrap();

        assert_eq!(model.resources.object.len(), 4);
        assert_eq!(
            model
                .resources
                .object
                .iter()
                .filter(|o| o.mesh.is_some())
                .count(),
            3
        );
        assert_eq!(
            model
                .resources
                .object
                .iter()
                .filter(|o| o.components.is_some())
                .count(),
            1
        );
        assert_eq!(model.build.item.len(), 2);
    }

    #[test]
    fn test_save_mesh_instances() {
        let threemf_file = File::open(PathBuf::from("../mesh-composedpart.3mf")).unwrap();
        let db = load_3mf::load(threemf_file).unwrap();

        //there should be 3 mesh instances in this db
        let mesh_instances = db
            .get_part_instances()
            .filter_map(|(id, _, part)| {
                if matches!(part.get_rep(), PartRep::Mesh(_)) {
                    Some(id)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let builder = save_instances(&db, &mesh_instances).unwrap();
        let model = builder.build().unwrap();

        assert_eq!(model.resources.object.len(), 3);
        assert_eq!(
            model
                .resources
                .object
                .iter()
                .filter(|o| o.mesh.is_some())
                .count(),
            3
        );
        assert_eq!(
            model
                .resources
                .object
                .iter()
                .filter(|o| o.components.is_some())
                .count(),
            0
        );
        assert_eq!(model.build.item.len(), 3);
    }

    #[test]
    fn test_save_composed_part_instances() {
        let threemf_file = File::open(PathBuf::from("../mesh-composedpart.3mf")).unwrap();
        let db = load_3mf::load(threemf_file).unwrap();

        // there should be 1 composed part instance
        let composedpart_instance = db
            .get_part_instances()
            .filter_map(|(id, _, part)| {
                if matches!(part.get_rep(), PartRep::ComposedPart(_)) {
                    Some(id)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let builder = save_instances(&db, &composedpart_instance).unwrap();
        let model = builder.build().unwrap();

        // 2 mesh + 1 components object
        assert_eq!(model.resources.object.len(), 3);
        assert_eq!(
            model
                .resources
                .object
                .iter()
                .filter(|o| o.mesh.is_some())
                .count(),
            2
        );
        assert_eq!(
            model
                .resources
                .object
                .iter()
                .filter(|o| o.components.is_some())
                .count(),
            1
        );

        // only the components object appears in build
        assert_eq!(model.build.item.len(), 1);
    }

    #[test]
    fn test_save_mesh_parts() {
        let threemf_file = File::open(PathBuf::from("../mesh-composedpart.3mf")).unwrap();
        let db = load_3mf::load(threemf_file).unwrap();

        //there should be 3 mesh instances in this db
        let mesh_parts = db
            .get_parts()
            .filter_map(|(id, part)| {
                if matches!(part.get_rep(), PartRep::Mesh(_)) {
                    Some(id)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let builder = save_parts(&db, &mesh_parts).unwrap();
        let model = builder.build().unwrap();

        assert_eq!(model.resources.object.len(), 3);
        assert_eq!(
            model
                .resources
                .object
                .iter()
                .filter(|o| o.mesh.is_some())
                .count(),
            3
        );
        assert_eq!(
            model
                .resources
                .object
                .iter()
                .filter(|o| o.components.is_some())
                .count(),
            0
        );
        assert_eq!(model.build.item.len(), 3);
    }

    #[test]
    fn test_save_composed_part_object() {
        let threemf_file = File::open(PathBuf::from("../mesh-composedpart.3mf")).unwrap();
        let db = load_3mf::load(threemf_file).unwrap();

        // there should be 1 composed part instance
        let composed_part = db
            .get_parts()
            .filter_map(|(id, part)| {
                if matches!(part.get_rep(), PartRep::ComposedPart(_)) {
                    Some(id)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let builder = save_parts(&db, &composed_part).unwrap();
        let model = builder.build().unwrap();

        // 2 mesh + 1 components object
        assert_eq!(model.resources.object.len(), 3);
        assert_eq!(
            model
                .resources
                .object
                .iter()
                .filter(|o| o.mesh.is_some())
                .count(),
            2
        );
        assert_eq!(
            model
                .resources
                .object
                .iter()
                .filter(|o| o.components.is_some())
                .count(),
            1
        );

        // only the components object appears in build
        assert_eq!(model.build.item.len(), 1);
    }
}
