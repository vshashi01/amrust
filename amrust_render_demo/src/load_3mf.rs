use async_trait::async_trait;
use glam::Vec3;
use smol::Timer;
use thiserror::Error;
use threemf2::io::query::{self};

use amrust_render::transformation::Transformation;
use threemf2::core::mesh::{Triangle, Vertex};
use threemf2::core::transform::Transform;
use threemf2::io::ThreemfPackage;

use crate::amrust_db::{Db, DbError, Mesh, PartId, PartRep, Scene};
use crate::commands::{Command, CommandCategory, CommandContext, CommandsService};
use crate::operation::{DbContext, Operation, OperationNature, OperationResponse};
use crate::operation_manager::OperationRequest;

use core::f32;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Error)]
pub enum DbFrom3mfError {
    #[error("Object not found: {0}")]
    ObjectNotFound(usize),

    #[error("Error with database")]
    DbError(#[from] DbError),

    #[error("Somethign wrong with threemf process")]
    ThreemfProcessingError(#[from] threemf2::io::Error),
}

pub struct Load3MFOps {
    pub path: PathBuf,
}

#[async_trait]
impl Operation for Load3MFOps {
    fn name(&self) -> &str {
        "Load Parts from 3MF File"
    }

    fn get_operation_requirements(&self) -> Option<OperationNature> {
        Some(OperationNature::AppendOnlyFromBackground)
    }

    async fn execute(&mut self, context: &mut DbContext) -> OperationResponse {
        let file = std::fs::File::open(&self.path);
        match file {
            Ok(threemf_file) => match load(threemf_file) {
                Ok(db) => {
                    // Timer::after(Duration::from_secs(8)).await;

                    if let Err(err) = context.append_db(db).await {
                        return OperationResponse::Failed {
                            name: "Load 3MF",
                            error: Box::new(err),
                            is_restore_db_required: false,
                        };
                    } else {
                        return OperationResponse::Succeeded {
                            name: "Load 3MF Operation",
                        };
                    }
                }
                Err(err) => OperationResponse::Failed {
                    name: "Load 3MF",
                    error: Box::new(err),
                    is_restore_db_required: false,
                },
            },
            Err(err) => OperationResponse::Failed {
                name: "Load 3MF",
                error: Box::new(err),
                is_restore_db_required: false,
            },
        }
    }
}

pub fn load(threemf: std::fs::File) -> Result<Db, DbFrom3mfError> {
    let package = ThreemfPackage::from_reader_with_memory_optimized_deserializer(threemf, true)?;

    get_db_from_3mf(&package)
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct PartRepIdentity {
    id: usize,
    path: Option<String>,
}

fn get_db_from_3mf(package: &ThreemfPackage) -> Result<Db, DbFrom3mfError> {
    let mut db = Db::new();
    let mut part_rep_map = HashMap::<PartRepIdentity, PartId>::new();

    let mesh_objects = query::get_mesh_objects(package).collect::<Vec<_>>();
    process_mesh_objects(&mut db, &mut part_rep_map, mesh_objects)?;

    let composed_parts = query::get_components_objects(package).collect::<Vec<_>>();
    process_composed_parts(&mut db, &mut part_rep_map, composed_parts)?;

    let mut parts_in_scene = vec![];

    // add the parts that actually needs to be on the scene
    for item in &package.root.build.item {
        if let Some(unique_part_id) = part_rep_map.get(&PartRepIdentity {
            id: item.objectid,
            path: item.path.clone(),
        }) {
            let transform = get_transformation(&item.transform);
            let instance_id =
                db.make_new_part_instance_from_part(unique_part_id, Some(transform.into()))?;
            parts_in_scene.push(instance_id);
        }
    }

    let scene = Scene::new_from_instances(parts_in_scene);

    db.add_scene(scene)?;

    Ok(db)
}

fn process_composed_parts(
    db: &mut Db,
    part_rep_map: &mut HashMap<PartRepIdentity, PartId>,
    composed_parts: Vec<query::ComponentsObjectRef<'_>>,
) -> Result<(), DbFrom3mfError> {
    let mut unprocessed_composed_parts_id = composed_parts.iter().map(|o| o.id).collect::<Vec<_>>();

    loop {
        if unprocessed_composed_parts_id.is_empty() {
            break;
        }

        for o in &composed_parts {
            if unprocessed_composed_parts_id.contains(&o.id) {
                let mut instances = vec![];
                let components = o.components().collect::<Vec<_>>();
                let num_of_comps = components.len();

                let all_components_processed = components.iter().all(|c| {
                    part_rep_map.contains_key(&PartRepIdentity {
                        id: c.objectid,
                        path: c.path_to_look_for.clone(),
                    })
                });

                if !all_components_processed {
                    // if the part_rep_map does not contain the comp_id already then
                    // its child component is not processed and registered yet, skip the whole composed part for now
                    continue;
                }

                for comp in &components {
                    if let Some(unique_part_id) = part_rep_map.get(&PartRepIdentity {
                        id: comp.objectid,
                        path: comp.path_to_look_for.clone(),
                    }) {
                        let transformation = get_transformation(&comp.transform);
                        let instance_id = db.make_new_part_instance_from_part(
                            unique_part_id,
                            Some(transformation.into()),
                        )?;
                        instances.push(instance_id);
                    } else {
                        return Err(DbFrom3mfError::ObjectNotFound(comp.objectid));
                    }
                }

                if instances.len() == num_of_comps
                    && !instances.is_empty()
                    && let Ok(unique_part_id) = db.add_part_rep(PartRep::ComposedPart(instances))
                {
                    let path = o
                        .origin_model_path
                        .map(|parent_path| parent_path.to_owned());
                    part_rep_map.insert(PartRepIdentity { id: o.id, path }, unique_part_id);

                    if let Some(pos) = unprocessed_composed_parts_id
                        .iter()
                        .position(|id| *id == o.id)
                    {
                        unprocessed_composed_parts_id.swap_remove(pos);
                    }
                }
            }
        }
    }

    Ok(())
}

fn process_mesh_objects(
    db: &mut Db,
    part_rep_map: &mut HashMap<PartRepIdentity, PartId>,
    mesh_objects: Vec<query::MeshObjectRef<'_>>,
) -> Result<(), DbFrom3mfError> {
    let _: () = for obj in mesh_objects {
        let mesh = get_amrust_mesh(obj.mesh())?;

        let unique_part_id = db.add_part_rep(PartRep::Mesh(Box::new(mesh)))?;
        let obj_path = obj.origin_model_path.map(|path| path.to_owned());
        part_rep_map.insert(
            PartRepIdentity {
                id: obj.id,
                path: obj_path,
            },
            unique_part_id,
        );
    };
    Ok(())
}

//returns unique part id in the db
fn get_amrust_mesh(m: &threemf2::core::mesh::Mesh) -> Result<Mesh, DbFrom3mfError> {
    let mesh = Mesh {
        vertices: convert_3mf_vertices_to_mesh_vertices(&m.vertices.vertex),
        triangles: convert_3mf_triangles_to_mesh_triangles(&m.triangles.triangle),
    };

    Ok(mesh)
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

fn get_transformation(transform: &Option<threemf2::core::transform::Transform>) -> Transformation {
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

/// Command for importing 3MF files
pub struct ImportPartCommand;

impl Command for ImportPartCommand {
    fn id(&self) -> &str {
        "import_part"
    }

    fn label(&self) -> &str {
        "Import Part"
    }

    fn category(&self) -> CommandCategory {
        CommandCategory::File
    }

    fn execute(&self, context: &mut CommandContext) {
        // Show file dialog with handler for importing 3MF files
        context.file_dialog_service.show_load_dialog(
            "3D Manufacturing Format",
            vec!["3mf"],
            |path: PathBuf, ctx: &mut CommandContext| {
                println!("File picked is: {:?}", path);

                if let Some(ext) = path.extension()
                    && ext == "3mf"
                {
                    let ops = Load3MFOps { path };
                    if let Err(err) = ctx
                        .operation_queue_tx
                        .send_blocking(OperationRequest::BackgroundOp(Box::new(ops)))
                    {
                        println!("Failed to queue import operation: {:?}", err);
                    }
                }
            },
        );
    }
}

/// Register all commands provided by the load_3mf module
pub fn register_commands(commands_service: &mut CommandsService) {
    commands_service.register_command(Box::new(ImportPartCommand));
}
