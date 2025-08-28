use std::fmt::{Debug, format};
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
    Mesh(Box<Mesh>),
    ComposedPart(Vec<usize>), //this is probably not great
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

pub struct Mesh {
    pub vertices: Vec<Vec3>,
    pub triangles: Vec<u32>,
    pub bbox: BoundingBox,
}

impl Debug for Mesh {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mesh")
            .field("vertices:", &self.vertices.len())
            .field("triangles:", &self.triangles.len())
            .field("bbox:", &self.bbox)
            .finish()
    }
}

#[derive(Debug)]
pub struct Db {
    //meshes: Vec<Mesh>,
    part_reps: Vec<PartRep>,
    unique_parts: Vec<UniquePart>,
    scene: Option<Scene>,
}

#[derive(Debug, Error)]
pub enum DbError {
    #[error("No valid part found with the id: {0}")]
    InvalidPartId(usize),

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
            part_reps: vec![],
            unique_parts: vec![],
            scene: None,
        }
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

    pub fn get_part_rep(&mut self, part: Part) -> Option<&PartRep> {
        let unique_part = self.unique_parts.get(part.unique_part);
        match unique_part {
            Some(part) => {
                let part_rep = &self.part_reps[part.0];
                Some(part_rep)
            }
            None => None,
        }
    }

    pub fn add_scene(&mut self, scene: Scene) {
        let _ = self.scene.insert(scene);
    }

    fn validate_part_rep(&self, part_rep: &PartRep) -> Result<bool, DbError> {
        match part_rep {
            PartRep::Mesh(_) => {}
            PartRep::ComposedPart(parts) => {
                for unique_part_id in parts {
                    let unique_part = self.unique_parts.get(*unique_part_id);
                    match unique_part {
                        Some(part) => return self.validate_part(part),
                        None => return Err(DbError::InvalidPartId(*unique_part_id)),
                    }
                }
            }
        }
        Ok(true)
    }

    fn validate_part(&self, part: &UniquePart) -> Result<bool, DbError> {
        Ok(self.unique_parts.contains(part))
    }
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
                            parts_in_scene.push(Part {
                                unique_part: part_id,
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
            list_of_unique_part_id_per_component.push(partids.1);
        } //else need to look at other objects
    }

    if !list_of_unique_part_id_per_component.is_empty() {
        if let Ok(registered) =
            db.add_part_rep(PartRep::ComposedPart(list_of_unique_part_id_per_component))
        {
            part_rep_map.insert(object_id, registered);

            return Some(registered.1);
        }
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
