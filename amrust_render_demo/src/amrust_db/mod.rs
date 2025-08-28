use std::{collections::HashMap, path::PathBuf};

use amrust_3mf::{
    core::{Triangle, Vertex, transform::Transform},
    io::ThreemfPackage,
};
use amrust_render::{bounding_box::BoundingBox, transformation::Transformation};
use glam::Vec3;
use thiserror::Error;

#[derive(Debug)]
pub enum PartRep {
    Mesh(usize),
    ComposedPart(Vec<usize>),
}

#[derive(PartialEq, Debug)]
pub struct Part {
    pub unique_part: usize,
    pub transform: Transformation,
}

#[derive(PartialEq, Eq, Debug)]
struct UniquePart(usize); //points to a part_rep_id

#[derive(Debug)]
pub struct Scene(Vec<Part>);

#[derive(Debug)]
pub struct Mesh {
    pub vertices: Vec<Vec3>,
    pub triangles: Vec<u32>,
    pub bbox: BoundingBox,
}

#[derive(Debug)]
pub struct Db {
    pub meshes: Vec<Mesh>,
    pub part_reps: Vec<PartRep>,
    unique_parts: Vec<UniquePart>,
    pub scene: Option<Scene>,
}

#[derive(Debug, Error)]
pub enum DbError {
    #[error("No valid mesh found with the id: {0}")]
    NoValidMeshFound(usize),

    #[error("An invalid part rep is passed")]
    InvalidPartRep,
}

#[derive(Debug, Error)]
pub enum DbFrom3mfError {
    #[error("Something went wrong: {0}")]
    Unspecified(String),
}

impl Db {
    pub fn new() -> Self {
        Db {
            meshes: vec![],
            part_reps: vec![],
            unique_parts: vec![],
            scene: None,
        }
    }

    pub fn add_mesh(&mut self, mesh: Mesh) -> usize {
        self.meshes.push(mesh);
        self.meshes.len() - 1
    }

    // returns the part rep ID and the unique part ID
    pub fn add_part_rep(&mut self, part_rep: PartRep) -> Result<(usize, usize), DbError> {
        let valid = self.validate_part_rep(&part_rep)?;
        if !valid {
            return Err(DbError::InvalidPartRep);
        }

        self.part_reps.push(part_rep);
        let part_rep_id = self.part_reps.len() - 1;
        self.unique_parts.push(UniquePart(part_rep_id));
        let unique_part_id = self.unique_parts.len() - 1;
        Ok((part_rep_id, unique_part_id))
    }

    pub fn add_scene(&mut self, scene: Scene) {
        let _ = self.scene.insert(scene);
    }

    fn validate_part_rep(&self, part_rep: &PartRep) -> Result<bool, DbError> {
        match part_rep {
            PartRep::Mesh(id) => {
                if self.meshes.len() < *id {
                    return Err(DbError::NoValidMeshFound(*id));
                }
            }
            PartRep::ComposedPart(parts) => {
                for unique_part_id in parts {
                    let unique_part = &self.unique_parts[*unique_part_id];
                    self.validate_part(unique_part)?;
                }
            }
        }
        Ok(true)
    }

    fn validate_part(&self, part: &UniquePart) -> Result<bool, DbError> {
        Ok(self.unique_parts.contains(part))
    }
}

struct Data {
    pub bbox: BoundingBox,
    pub transforms: Vec<Transformation>,
}
pub fn get_db_from_3mf(filepath: PathBuf) -> Result<Db, DbFrom3mfError> {
    let threemf = std::fs::File::open(filepath).unwrap();
    let package = ThreemfPackage::from_reader(threemf, true).unwrap();

    let mut db = Db::new();
    let mut items_transform_pair = vec![];
    let mut part_rep_map = HashMap::<usize, (usize, usize)>::new(); //object id to part_rep id map
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
    for item in items_transform_pair {
        if let Some(ids) = part_rep_map.get_key_value(&item.0) {
            parts_in_scene.push(Part {
                unique_part: ids.1.1, //set the unique part id
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
                    if let Some(m) = &object.mesh {
                        let mesh = Mesh {
                            vertices: convert_3mf_vertices_to_mesh_vertices(&m.vertices.vertex),
                            triangles: convert_3mf_triangles_to_mesh_triangles(
                                &m.triangles.triangle,
                            ),
                            bbox: generate_bbox(&m.vertices.vertex),
                        };

                        let mesh_id = db.add_mesh(mesh);
                        if let Ok(registered) = db.add_part_rep(PartRep::Mesh(mesh_id)) {
                            part_rep_map.insert(object.id, registered);

                            parts_in_scene.push(Part {
                                unique_part: registered.1,
                                transform: item.1,
                            });
                        }
                    } else if let Some(comps) = &object.components {
                    }
                }
                None => {
                    return Err(DbFrom3mfError::Unspecified(
                        "Invalid 3mf Object Id".to_owned(),
                    ));
                }
            }
        }
    }

    db.add_scene(Scene(parts_in_scene));

    Ok(db)
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
