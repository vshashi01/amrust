use glam::{Mat4, Vec3};
use thiserror::Error;

use amrust_render::{
    RenderObject, bounding_box::BoundingBox, gpu_mesh::MeshBuilder, instance::InstanceDataBuilder,
    material::Material, renderer, transformation::Transformation, vertex::Position,
};

use std::collections::HashMap;
use std::fmt::Debug;

#[derive(Debug)]
pub enum PartRep {
    Mesh(Box<Mesh>),
    ComposedPart(Vec<PartInstance>),
}

#[derive(Debug)]
pub struct PartInstance {
    pub part_id: UniquePartId,
    pub transform: Transformation,
}

#[derive(PartialEq, Debug)]
pub struct Part(usize); //points to a part_rep_id

#[derive(Debug)]
pub struct Scene(pub Vec<PartInstance>);

pub struct Mesh {
    pub vertices: Vec<Vec3>,
    pub triangles: Vec<u32>,
}

impl Debug for Mesh {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mesh")
            .field("vertices:", &self.vertices.len())
            .field("triangles:", &self.triangles.len())
            // .field("bbox:", &self.bbox)
            .finish()
    }
}

// strong type to get a UniquePartReference
#[derive(Debug, Clone, Copy)]
pub struct UniquePartId(usize);

// strong type to get a PartRepReference
#[derive(Debug, Clone, Copy)]
pub struct PartRepId(usize);

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

impl Db {
    pub fn new() -> Self {
        Db {
            part_reps: vec![],
            unique_parts: vec![],
            scene: None,
        }
    }

    // returns the part rep ID and the unique part ID
    pub fn add_part_rep(
        &mut self,
        part_rep: PartRep,
    ) -> Result<(PartRepId, UniquePartId), DbError> {
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

        Ok((PartRepId(part_rep_id), UniquePartId(unique_part_id)))
    }

    pub fn get_part_rep(&self, part_instance: &PartInstance) -> Result<&PartRep, DbError> {
        self.get_part_rep_from_part(part_instance.part_id.0)
    }

    pub fn get_part_rep_from_part(&self, part_id: usize) -> Result<&PartRep, DbError> {
        let unique_part = self.unique_parts.get(part_id);
        match unique_part {
            Some(part) => {
                let part_rep = &self.part_reps[part.0];
                Ok(part_rep)
            }
            None => Err(DbError::PartIdNotFound(part_id)),
        }
    }

    pub fn get_new_part_instance_from_part_id(
        &self,
        part_id: UniquePartId,
        transform: Transformation,
    ) -> Result<PartInstance, DbError> {
        let unique_part = self.unique_parts.get(part_id.0);
        match unique_part {
            Some(_) => {
                let instance = PartInstance { part_id, transform };
                Ok(instance)
            }
            None => Err(DbError::PartIdNotFound(part_id.0)),
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
        Ok(part_instance.part_id.0 < self.unique_parts.len())
    }
}

struct InstanceData {
    pub gpu_mesh_id: u32,
    pub transforms: Vec<Transformation>,
}

pub fn add_render_items_from_db(
    renderer: &mut renderer::Renderer,
    db: &Db,
) -> Result<BoundingBox, DbError> {
    let scene = db.get_scene();
    let mut total_bbox = BoundingBox::default();

    if let Ok(scene) = scene {
        let mut part_id_to_instance_data = HashMap::<usize, InstanceData>::new();
        let mut bboxes = vec![];

        //setup all the instance data
        for instance in &scene.0 {
            match process_part_instance(
                renderer,
                db,
                &mut part_id_to_instance_data,
                instance,
                &Transformation(Mat4::IDENTITY),
            ) {
                Ok(_) => {}
                Err(err) => return Err(err),
            }

            match compute_instance_bbox(db, instance, &Transformation(Mat4::IDENTITY)) {
                Ok(bbox) => bboxes.push(bbox),
                Err(err) => return Err(err),
            }
        }

        part_id_to_instance_data
            .into_iter()
            .for_each(|(_, instance_data)| {
                add_render_object(renderer, &instance_data);
            });

        bboxes.iter().for_each(|bbox| {
            add_bounding_box_wireframe(renderer, bbox);
            total_bbox.unite(bbox)
        });
    }

    add_bounding_box_wireframe(renderer, &total_bbox);
    println!("Total BBOX is {:?}", total_bbox);

    Ok(total_bbox)
}

fn process_part_instance(
    renderer: &mut renderer::Renderer,
    db: &Db,
    part_id_to_instance_data: &mut HashMap<usize, InstanceData>,
    instance: &PartInstance,
    parent_transform: &Transformation,
) -> Result<(), DbError> {
    let combined_transform = Transformation(parent_transform.0 * instance.transform.0);
    //if the necessary part is already created then just push new transform data to add an additional render object
    if let Some(instance_data) = part_id_to_instance_data.get_mut(&instance.part_id.0) {
        instance_data.transforms.push(combined_transform);
        return Ok(());
    }

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
                part_id_to_instance_data.insert(
                    instance.part_id.0,
                    InstanceData {
                        gpu_mesh_id,
                        transforms: vec![combined_transform],
                    },
                );
            }
            PartRep::ComposedPart(part_instances) => {
                for i in part_instances {
                    match process_part_instance(
                        renderer,
                        db,
                        part_id_to_instance_data,
                        i,
                        &combined_transform,
                    ) {
                        Ok(_) => {}
                        Err(err) => return Err(err),
                    }
                }
            }
        },
        Err(err) => return Err(err),
    }
    Ok(())
}

fn add_render_object(renderer: &mut renderer::Renderer, data: &InstanceData) {
    let transformation_data = data
        .transforms
        .iter()
        .map(|t| t.to_data())
        .collect::<Vec<_>>();
    let material_data = vec![Material::new(1.0, 1.0, 1.0).to_data(); transformation_data.len()];
    let object = RenderObject {
        renderable: amrust_render::Renderable::ColoredMesh(data.gpu_mesh_id),
        instance: InstanceDataBuilder::new()
            .add_instance_stream(transformation_data.as_slice())
            .add_instance_stream(material_data.as_slice())
            .build(&renderer.device),
    };
    let _ = renderer.add_object(object);

    // println!("Colored Object id {}", object_id);

    let wireframe_object = RenderObject {
        renderable: amrust_render::Renderable::WireframeMesh(data.gpu_mesh_id),
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

fn compute_instance_bbox(
    db: &Db,
    instance: &PartInstance,
    parent_transform: &Transformation,
) -> Result<BoundingBox, DbError> {
    let combined_transform = Transformation(parent_transform.0 * instance.transform.0);
    let part_rep = db.get_part_rep(instance)?;

    match part_rep {
        PartRep::Mesh(mesh) => {
            let bbox = compute_transformed_bounding_box_from_mesh(mesh, &combined_transform);
            Ok(bbox)
        }
        PartRep::ComposedPart(children) => {
            let mut bbox = BoundingBox::default();
            for child in children {
                let child_bbox = compute_instance_bbox(db, child, &combined_transform)?;
                bbox.unite(&child_bbox);
            }
            Ok(bbox)
        }
    }
}

fn compute_transformed_bounding_box_from_mesh(
    mesh: &Mesh,
    transform: &Transformation,
) -> BoundingBox {
    let mut bbox = BoundingBox::default();
    for v in &mesh.vertices {
        let v4 = transform.0 * v.extend(1.0);
        let transformed = Vec3::new(v4.x, v4.y, v4.z);
        bbox.expand_to_include(&transformed);
    }
    bbox
}
