use glam::{Mat4, Vec3};
use thiserror::Error;

use amrust_3mf::{
    core::{Triangle, Vertex, transform::Transform},
    io::ThreemfPackage,
};
use amrust_render::{
    RenderObject, bounding_box::BoundingBox, gpu_mesh::MeshBuilder, instance::InstanceDataBuilder,
    material::Material, renderer, transformation::Transformation, vertex::Position,
};

use std::fmt::Debug;
use std::{collections::HashMap, path::PathBuf};

#[derive(Debug)]
pub enum PartRep {
    Mesh(Box<Mesh>),
    ComposedPart(Vec<PartInstance>), //this is probably not great
}

#[derive(Debug)]
pub struct PartInstance {
    pub part_id: usize,
    pub transform: Transformation,
}

#[derive(PartialEq, Debug)]
pub struct Part(usize); //points to a part_rep_id

#[derive(Debug)]
pub struct Scene(Vec<PartInstance>);

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
    part_reps: Vec<PartRep>,
    unique_parts: Vec<Part>,
    scene: Option<Scene>,
}

#[derive(Debug, Error)]
pub enum DbError {
    #[error("No valid part found with the id: {0}")]
    PartIdNotFound(usize),

    #[error("An invalid part rep is passed")]
    InvalidPartRep,

    #[error("Invalid Part Instances found")]
    PartInstancesNotFound(Vec<PartInstance>),

    #[error("There is no scene set currently")]
    SceneNotSet,
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

        //create part rep
        self.part_reps.push(part_rep);
        let part_rep_id = self.part_reps.len() - 1;

        //create unique part
        self.unique_parts.push(Part(part_rep_id));
        let unique_part_id = self.unique_parts.len() - 1;

        Ok((part_rep_id, unique_part_id))
    }

    pub fn get_part_rep(&self, part_instance: &PartInstance) -> Result<&PartRep, DbError> {
        let unique_part = self.unique_parts.get(part_instance.part_id);
        match unique_part {
            Some(part) => {
                let part_rep = &self.part_reps[part.0];
                Ok(part_rep)
            }
            None => Err(DbError::PartIdNotFound(part_instance.part_id)),
        }
    }

    pub fn get_new_part_instance_from_part_id(
        &self,
        part_id: usize,
        transform: Transformation,
    ) -> Result<PartInstance, DbError> {
        let unique_part = self.unique_parts.get(part_id);
        match unique_part {
            Some(_) => {
                let instance = PartInstance { part_id, transform };
                Ok(instance)
            }
            None => Err(DbError::PartIdNotFound(part_id)),
        }
    }

    pub fn add_scene(&mut self, scene: Scene) {
        let _ = self.scene.insert(scene);
    }

    pub fn add_part_instance_to_scene(
        &mut self,
        part_instance: PartInstance,
    ) -> Result<bool, DbError> {
        let valid = self.validate_part_instance(&part_instance)?;
        if valid {
            match &mut self.scene {
                Some(scene) => {
                    scene.0.push(part_instance);
                    Ok(true)
                }
                None => {
                    self.scene.get_or_insert(Scene(vec![part_instance]));
                    Ok(true)
                }
            }
        } else {
            Ok(false)
        }
    }

    pub fn get_scene(&self) -> Result<&Scene, DbError> {
        match &self.scene {
            Some(scene) => Ok(scene),
            None => Err(DbError::SceneNotSet),
        }
    }

    fn validate_part_rep(&self, part_rep: &PartRep) -> Result<bool, DbError> {
        match part_rep {
            PartRep::Mesh(_) => {}
            PartRep::ComposedPart(part_instances) => {
                let mut invalid_part_instances = vec![];
                for instance in part_instances {
                    let validated = self.validate_part_instance(instance);
                    match validated {
                        Ok(valid) => {
                            if !valid {
                                invalid_part_instances.push(instance);
                            }
                        }
                        Err(err) => return Err(err),
                    }
                }

                if !invalid_part_instances.is_empty() {
                    let invalid_instances = invalid_part_instances
                        .iter()
                        .map(|i| PartInstance {
                            part_id: i.part_id,
                            transform: i.transform,
                        })
                        .collect();
                    return Err(DbError::PartInstancesNotFound(invalid_instances));
                }
            }
        }
        Ok(true)
    }

    fn validate_part_instance(&self, part_instance: &PartInstance) -> Result<bool, DbError> {
        //ToDo: introduce some unique identifier for PartInstance to Part tracking
        Ok(part_instance.part_id < self.unique_parts.len())
    }
}

struct RepData {
    pub gpu_mesh_id: u32,
    pub bbox: BoundingBox,
}

struct InstanceData {
    pub transforms: Vec<Transformation>,
}

pub fn add_render_items_from_db(
    renderer: &mut renderer::Renderer,
    db: &Db,
) -> Result<BoundingBox, DbError> {
    let scene = db.get_scene();
    let mut total_bbox = BoundingBox::default();

    if let Ok(scene) = scene {
        let mut mesh_rep_gpu_mesh_map = HashMap::<usize, RepData>::new(); //part_id to gpu_mesh_id
        let mut part_id_instance_data = HashMap::<usize, InstanceData>::new();

        //setup all the instance data
        for instance in &scene.0 {
            if let Some(instance_data) = part_id_instance_data.get_mut(&instance.part_id) {
                instance_data.transforms.push(instance.transform);
            } else {
                let part_rep = db.get_part_rep(instance);
                match part_rep {
                    Ok(rep) => match rep {
                        PartRep::Mesh(mesh) => {
                            let positions = convert_vertices_to_position(&mesh.vertices);
                            // println!("Number of vertices: {}", positions.len());
                            let indices = mesh.triangles.clone();
                            // println!("Number of triangles: {}", indices.len() / 3);
                            let color = convert_vertices_to_color(&mesh.vertices);
                            let wireframe_indices =
                                convert_triangle_indices_to_wireframe_indices(&mesh.triangles);

                            let gpu_mesh = MeshBuilder::new()
                                .add_vertex_stream(positions.as_slice())
                                .add_vertex_stream(color.as_slice())
                                .add_mesh_index_stream(indices.as_slice())
                                .add_wireframe_index_stream(wireframe_indices.as_slice())
                                .build(&renderer.device);

                            let gpu_mesh_id = renderer.add_mesh(gpu_mesh);

                            mesh_rep_gpu_mesh_map.insert(
                                instance.part_id,
                                RepData {
                                    gpu_mesh_id,
                                    bbox: mesh.bbox.clone(),
                                },
                            );

                            part_id_instance_data.insert(
                                instance.part_id,
                                InstanceData {
                                    transforms: vec![instance.transform],
                                },
                            );
                        }
                        PartRep::ComposedPart(part_instances) => {}
                    },
                    Err(err) => return Err(err),
                }
            }
        }

        //add the instance data to the renderer
        for (key, value) in part_id_instance_data {
            let rep_data = &mesh_rep_gpu_mesh_map[&key];
            add_render_object(renderer, &value, rep_data.gpu_mesh_id);

            let mut instance_bbox = BoundingBox::default();
            //calculate bounding box
            for transform in &value.transforms {
                let mut default_bbox = rep_data.bbox.clone();
                default_bbox.transform(transform);
                instance_bbox.unite(&default_bbox);
            }

            total_bbox.unite(&instance_bbox);
        }
    }

    add_bounding_box_wireframe(renderer, &total_bbox);

    Ok(total_bbox)
}

fn add_render_object(renderer: &mut renderer::Renderer, data: &InstanceData, gpu_mesh_id: u32) {
    let transformation_data = data
        .transforms
        .iter()
        .map(|t| t.to_data())
        .collect::<Vec<_>>();
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

fn convert_vertices_to_position(vertices: &[Vec3]) -> Vec<Position> {
    vertices.iter().map(|v| Position([v.x, v.y, v.z])).collect()
}

fn convert_vertices_to_color(vertices: &[Vec3]) -> Vec<amrust_render::vertex::Color> {
    vertices
        .iter()
        .map(|_| amrust_render::vertex::Color([0.5, 0.5, 0.5]))
        .collect()
}

fn convert_triangle_indices_to_wireframe_indices(triangles: &[u32]) -> Vec<u32> {
    let mut indices = Vec::new();

    for triangle in triangles.chunks(3) {
        if triangle.len() == 3 {
            let v1 = triangle[0];
            let v2 = triangle[1];
            let v3 = triangle[2];
            indices.push(v1);
            indices.push(v2);
            indices.push(v2);
            indices.push(v3);
            indices.push(v3);
            indices.push(v1);
        }
    }
    indices
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
