use core::f32;

use crate::core::amrust_db::Db;
use crate::core::app_mode::AppMode;
use crate::core::types::identifiable::Identifiable;
use amrust_render::bounding_box::BoundingBox;
use glam::{Mat4, Vec2, Vec3};

#[derive(Debug, Clone)]
pub struct PickedEntity {
    pub entity: Identifiable,
    pub intersection: Vec3,
    pub distance: f32,
}

#[derive(Debug, Clone)]
pub struct Ray {
    pub origin: Vec3,
    pub direction: Vec3,
}

pub struct PickerService;

impl PickerService {
    pub fn pick(
        &self,
        db: &Db,
        app_mode: &AppMode,
        screen_point: Vec2,
        viewport_size: Vec2,
        view_proj: Mat4,
        radius: f32,
    ) -> Vec<PickedEntity> {
        let ray = screen_to_ray(screen_point, viewport_size, view_proj);
        let mut results = Vec::new();

        match app_mode {
            AppMode::Build => {
                if let Ok(scene) = db.get_scene() {
                    for &instance_id in &scene.instances {
                        if let Ok(instance) = db.get_part_instance_data(&instance_id)
                            && let Ok(part) = db.get_part_data(&instance.part_id)
                            && let Some(mesh) = part.get_rep().as_mesh()
                        {
                            let transformed_bbox =
                                compute_transformed_bbox(mesh, &instance.transform);
                            if !ray_bbox_intersect(&ray, &transformed_bbox) {
                                continue;
                            }
                            let local_ray = transform_ray_to_local(&ray, &instance.transform);
                            let intersections = intersect_ray_mesh(&local_ray, mesh);
                            for intersection in intersections {
                                // let world_intersection =
                                //     instance.transform.0.transform_point3(intersection);

                                let perp_dist = distance_point_to_ray(&intersection, &ray);
                                if perp_dist <= radius {
                                    let distance = distance_along_ray(&intersection, &ray);
                                    results.push(PickedEntity {
                                        entity: Identifiable::PartInstance(instance_id),
                                        intersection,
                                        distance,
                                    });
                                }
                            }
                        }
                    }
                }
            }
            AppMode::Objects => {
                for (part_id, part) in db.get_parts() {
                    if let Some(mesh) = part.get_rep().as_mesh() {
                        let bbox = compute_mesh_bbox(mesh);
                        if !ray_bbox_intersect(&ray, &bbox) {
                            continue;
                        }
                        let intersections = intersect_ray_mesh(&ray, mesh);
                        for intersection in intersections {
                            let perp_distance = distance_point_to_ray(&intersection, &ray);
                            if perp_distance <= radius {
                                let distance = distance_along_ray(&intersection, &ray);
                                if distance <= radius {
                                    results.push(PickedEntity {
                                        entity: Identifiable::Part(part_id),
                                        intersection,
                                        distance,
                                    });
                                }
                            }
                        }
                    }
                    // For composed parts, recurse on instances
                    if let Some(composed) = part.get_rep().as_composed_part() {
                        for &child_instance_id in composed {
                            if let Ok(child_instance) =
                                db.get_part_instance_data(&child_instance_id)
                                && let Ok(child_part) = db.get_part_data(&child_instance.part_id)
                                && let Some(child_mesh) = child_part.get_rep().as_mesh()
                            {
                                let combined_transform = child_instance.transform; // Assuming no parent transform here
                                let transformed_bbox =
                                    compute_transformed_bbox(child_mesh, &combined_transform);
                                if !ray_bbox_intersect(&ray, &transformed_bbox) {
                                    continue;
                                }
                                let local_ray = transform_ray_to_local(&ray, &combined_transform);
                                let intersections = intersect_ray_mesh(&local_ray, child_mesh);
                                for intersection in intersections {
                                    // let world_intersection =
                                    //     combined_transform.0.transform_point3(intersection);

                                    let perp_distance = distance_point_to_ray(&intersection, &ray);
                                    if perp_distance <= radius {
                                        let distance = distance_along_ray(&intersection, &ray);
                                        if distance <= radius {
                                            results.push(PickedEntity {
                                                entity: Identifiable::PartInstance(
                                                    child_instance_id,
                                                ),
                                                intersection,
                                                distance,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        results.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap());
        results
    }
}

fn screen_to_ray(screen_point: Vec2, viewport_size: Vec2, view_proj: Mat4) -> Ray {
    // Normalize to NDC (-1 to 1)
    let ndc_x = (2.0 * screen_point.x / viewport_size.x) - 1.0;
    let ndc_y = 1.0 - (2.0 * screen_point.y / viewport_size.y);

    // Orthographic: ray direction is constant, origin varies
    // let view_proj = camera.get_view_matrix() * camera.get_projection_matrix();
    let inv_view_proj = view_proj.inverse();

    let near_point = inv_view_proj * Vec3::new(ndc_x, ndc_y, -1.0).extend(1.0);
    let far_point = inv_view_proj * Vec3::new(ndc_x, ndc_y, 1.0).extend(1.0);

    let near_world = near_point.truncate() / near_point.w;
    let far_world = far_point.truncate() / far_point.w;

    Ray {
        origin: near_world,
        direction: (far_world - near_world).normalize(),
    }
}

// fn ray_bbox_intersect(ray: &Ray, bbox: &BoundingBox) -> bool {
//     let min = bbox.min;
//     let max = bbox.max;

//     let inv_dir = Vec3::new(
//         1.0 / ray.direction.x,
//         1.0 / ray.direction.y,
//         1.0 / ray.direction.z,
//     );

//     let t1 = (min.x - ray.origin.x) * inv_dir.x;
//     let t2 = (max.x - ray.origin.x) * inv_dir.x;
//     let t3 = (min.y - ray.origin.y) * inv_dir.y;
//     let t4 = (max.y - ray.origin.y) * inv_dir.y;
//     let t5 = (min.z - ray.origin.z) * inv_dir.z;
//     let t6 = (max.z - ray.origin.z) * inv_dir.z;

//     let tmin = t1.min(t2).max(t3.min(t4)).max(t5.min(t6));
//     let tmax = t1.max(t2).min(t3.max(t4)).min(t5.max(t6));

//     tmax >= tmin && tmax >= 0.0
// }

fn ray_bbox_intersect(ray: &Ray, bbox: &BoundingBox) -> bool {
    let min = bbox.min;
    let max = bbox.max;

    let mut tmin = f32::NEG_INFINITY;
    let mut tmax = f32::INFINITY;

    for i in 0..3 {
        let origin = ray.origin[i];
        let dir = ray.direction[i];

        if dir.abs() < 1e-8 {
            if origin < min[i] || origin > max[i] {
                return false;
            }
        } else {
            let inv_dir = 1.0 / dir;
            let mut t1 = (min[i] - origin) * inv_dir;
            let mut t2 = (max[i] - origin) * inv_dir;

            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
            }

            tmin = tmin.max(t1);
            tmax = tmax.min(t2);

            if tmax < tmin {
                return false;
            }
        }
    }

    tmax >= 0.0
}

fn intersect_ray_mesh(ray: &Ray, mesh: &crate::core::types::mesh::Mesh) -> Vec<Vec3> {
    let mut intersections = Vec::new();
    let triangles = mesh.triangles.chunks_exact(3);
    for triangle in triangles {
        let v0 = mesh.vertices[triangle[0] as usize];
        let v1 = mesh.vertices[triangle[1] as usize];
        let v2 = mesh.vertices[triangle[2] as usize];
        if let Some(intersection) = ray_triangle_intersect(ray, v0, v1, v2) {
            intersections.push(intersection);
        }
    }
    intersections
}

fn ray_triangle_intersect(ray: &Ray, v0: Vec3, v1: Vec3, v2: Vec3) -> Option<Vec3> {
    // Moller-Trumbore algorithm
    let edge1 = v1 - v0;
    let edge2 = v2 - v0;
    let h = ray.direction.cross(edge2);
    let a = edge1.dot(h);
    if a.abs() < 1e-6 {
        return None; // Parallel
    }
    let f = 1.0 / a;
    let s = ray.origin - v0;
    let u = f * s.dot(h);
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(edge1);
    let v = f * ray.direction.dot(q);
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = f * edge2.dot(q);
    if t > 1e-6 {
        Some(ray.origin + ray.direction * t)
    } else {
        None
    }
}

fn distance_point_to_ray(point: &Vec3, ray: &Ray) -> f32 {
    let to_point = *point - ray.origin;
    let proj = to_point.dot(ray.direction);
    let closest = ray.origin + ray.direction * proj;
    (*point - closest).length()
}

fn distance_along_ray(point: &Vec3, ray: &Ray) -> f32 {
    (point - ray.origin).dot(ray.direction)
}

fn compute_mesh_bbox(mesh: &crate::core::types::mesh::Mesh) -> BoundingBox {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for &v in &mesh.vertices {
        min = min.min(v);
        max = max.max(v);
    }
    BoundingBox { min, max }
}

fn compute_transformed_bbox(
    mesh: &crate::core::types::mesh::Mesh,
    transform: &crate::core::types::transformation::Transformation,
) -> BoundingBox {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for &v in &mesh.vertices {
        let tv = transform.0.transform_point3(v);
        min = min.min(tv);
        max = max.max(tv);
    }
    BoundingBox { min, max }
}

fn transform_ray_to_local(
    ray: &Ray,
    transform: &crate::core::types::transformation::Transformation,
) -> Ray {
    let inv_transform = transform.0.inverse();
    let origin = inv_transform.transform_point3(ray.origin);
    let direction = inv_transform.transform_vector3(ray.direction).normalize();
    Ray { origin, direction }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::amrust_db::Db;
    use crate::core::app_mode::AppMode;

    #[test]
    fn test_pick_empty_db() {
        let picker = PickerService;
        let db = Db::new();
        let app_mode = AppMode::Build;
        let screen_point = glam::Vec2::new(0.0, 0.0);
        let viewport_size = glam::Vec2::new(800.0, 600.0);
        let radius = 0.1;

        let results = picker.pick(
            &db,
            &app_mode,
            screen_point,
            viewport_size,
            Mat4::IDENTITY,
            radius,
        );
        assert!(results.is_empty());
    }

    #[test]
    fn test_screen_to_ray() {
        let screen_point = glam::Vec2::new(400.0, 300.0); // center
        let viewport_size = glam::Vec2::new(800.0, 600.0);

        let ray = screen_to_ray(screen_point, viewport_size, Mat4::IDENTITY);
        // For orthographic, direction should be along Z or something, but depends on camera.
        // Just check it's a ray
        assert!(ray.direction.length() > 0.0);
    }

    #[test]
    fn test_ray_bbox_intersect() {
        let ray = Ray {
            origin: glam::Vec3::new(0.0, 0.0, 0.0),
            direction: glam::Vec3::new(0.0, 0.0, 1.0),
        };
        let bbox = amrust_render::bounding_box::BoundingBox {
            min: glam::Vec3::new(-1.0, -1.0, 0.5),
            max: glam::Vec3::new(1.0, 1.0, 1.5),
        };

        assert!(ray_bbox_intersect(&ray, &bbox));
    }
}
