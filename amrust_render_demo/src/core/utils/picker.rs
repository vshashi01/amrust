use crate::core::types::identifiable::Identifiable;
use crate::core::types::part::{PartId, PartRep};
use crate::core::types::transformation::Transformation;
use crate::core::{amrust_db::Db, types::part_instance::PartInstanceId};
use amrust_render::bounding_box::BoundingBox;
use glam::{Mat4, Vec2, Vec3};

use core::f32;

#[derive(Debug, Clone)]
pub struct PickedEntity {
    pub entity: Identifiable,
    pub intersection: Vec3,
    pub distance: f32,
}

#[derive(Debug, Clone)]
struct Ray {
    pub origin: Vec3,
    pub direction: Vec3,
}

pub struct PickerConfig {
    pub screen_pt: glam::Vec2,
    pub viewport_size: glam::Vec2,
    pub view_proj: glam::Mat4,
    pub snap_radius: f32,
    pub return_all_intersections: bool,
}

pub struct Picker;

impl Picker {
    pub fn pick_from_instances(
        &self,
        config: &PickerConfig,
        db: &Db,
        instances_can_be_picked: &[PartInstanceId],
        skip_child_instances: bool,
        parent_transform: Transformation,
    ) -> Vec<PickedEntity> {
        let ray = screen_to_ray(config.screen_pt, config.viewport_size, config.view_proj);
        let mut results = Vec::new();

        for instance_id in instances_can_be_picked {
            if let Ok(instance) = db.get_part_instance_data(instance_id)
                && let Ok(part) = db.get_part_data(&instance.part_id)
            {
                let combined_transform = Transformation(parent_transform.0 * instance.transform.0);

                match part.get_rep() {
                    PartRep::Mesh(mesh) => {
                        let transformed_bbox = compute_transformed_bbox(mesh, &combined_transform);
                        if !ray_bbox_intersect(&ray, &transformed_bbox) {
                            continue;
                        }
                        let local_ray = transform_ray_to_local(&ray, &combined_transform);
                        let intersections = intersect_ray_mesh(&local_ray, mesh);

                        let mut closest_hit: Option<(f32, Vec3)> = None;
                        let mut intermediate_results = vec![];
                        for intersection in intersections {
                            let world_intersection =
                                combined_transform.0.transform_point3(intersection);
                            let perp_dist = distance_point_to_ray(&world_intersection, &ray);
                            if perp_dist <= config.snap_radius {
                                let distance = distance_along_ray(&world_intersection, &ray);
                                intermediate_results.push(PickedEntity {
                                    entity: Identifiable::PartInstance(*instance_id),
                                    intersection: world_intersection,
                                    distance,
                                });

                                if let Some(prev_hit) = closest_hit.take() {
                                    if distance < prev_hit.0 {
                                        let _ = closest_hit.insert((distance, world_intersection));
                                    } else {
                                        let _ = closest_hit.insert(prev_hit);
                                    }
                                } else {
                                    let _ = closest_hit.insert((distance, world_intersection));
                                }
                            }
                        }

                        if !intermediate_results.is_empty() && config.return_all_intersections {
                            results.append(&mut intermediate_results);
                        } else if let Some(closest_hit) = closest_hit {
                            results.push(PickedEntity {
                                entity: Identifiable::PartInstance(*instance_id),
                                intersection: closest_hit.1,
                                distance: closest_hit.0,
                            });
                        }
                    }
                    PartRep::ComposedPart(part_instance_ids) => {
                        let mut picked_instances = self.pick_from_instances(
                            config,
                            db,
                            part_instance_ids,
                            skip_child_instances,
                            combined_transform,
                        );

                        //ToDo:: Replace this with something more sensical
                        if !picked_instances.is_empty() {
                            if let Some(picked) = picked_instances.first() {
                                results.push(PickedEntity {
                                    entity: Identifiable::PartInstance(*instance_id),
                                    intersection: picked.intersection,
                                    distance: picked.distance,
                                });
                            }

                            if !skip_child_instances {
                                results.append(&mut picked_instances);
                            }
                        }
                    }
                }
            }
        }

        results.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap());
        results
    }

    pub fn pick_from_parts(
        &self,
        config: &PickerConfig,
        db: &Db,
        parts_can_be_picked: &[PartId],
        skip_child_instances: bool,
    ) -> Vec<PickedEntity> {
        let ray = screen_to_ray(config.screen_pt, config.viewport_size, config.view_proj);
        let mut results = Vec::new();

        for part_id in parts_can_be_picked {
            if let Ok(part) = db.get_part_data(part_id) {
                match part.get_rep() {
                    PartRep::Mesh(mesh) => {
                        let bbox = compute_mesh_bbox(mesh);
                        if !ray_bbox_intersect(&ray, &bbox) {
                            continue;
                        }
                        let intersections = intersect_ray_mesh(&ray, mesh);

                        let mut closest_hit: Option<(f32, Vec3)> = None;
                        let mut intermediate_results = vec![];
                        for intersection in intersections {
                            let perp_dist = distance_point_to_ray(&intersection, &ray);
                            if perp_dist <= config.snap_radius {
                                let distance = distance_along_ray(&intersection, &ray);
                                intermediate_results.push(PickedEntity {
                                    entity: Identifiable::Part(*part_id),
                                    intersection,
                                    distance,
                                });

                                if let Some(prev_hit) = closest_hit.take() {
                                    if distance < prev_hit.0 {
                                        let _ = closest_hit.insert((distance, intersection));
                                    } else {
                                        let _ = closest_hit.insert(prev_hit);
                                    }
                                } else {
                                    let _ = closest_hit.insert((distance, intersection));
                                }
                            }
                        }

                        if !intermediate_results.is_empty() && config.return_all_intersections {
                            results.append(&mut intermediate_results);
                        } else if let Some(closest_hit) = closest_hit {
                            results.push(PickedEntity {
                                entity: Identifiable::Part(*part_id),
                                intersection: closest_hit.1,
                                distance: closest_hit.0,
                            });
                        }
                    }
                    PartRep::ComposedPart(part_instance_ids) => {
                        let mut picked_instances = self.pick_from_instances(
                            config,
                            db,
                            part_instance_ids,
                            false,
                            Transformation(Mat4::IDENTITY),
                        );

                        //ToDo:: Replace this with something more sensical
                        if !picked_instances.is_empty() {
                            if let Some(picked) = picked_instances.first() {
                                results.push(PickedEntity {
                                    entity: Identifiable::Part(*part_id),
                                    intersection: picked.intersection,
                                    distance: picked.distance,
                                });
                            }

                            if !skip_child_instances {
                                results.append(&mut picked_instances);
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

    // #[test]
    // fn test_pick_empty_db() {
    //     let picker = PickerService;
    //     let db = Db::new();
    //     let app_mode = AppMode::Build;
    //     let screen_point = glam::Vec2::new(0.0, 0.0);
    //     let viewport_size = glam::Vec2::new(800.0, 600.0);
    //     let radius = 0.1;

    //     let results = picker.pick(
    //         &db,
    //         &app_mode,
    //         screen_point,
    //         viewport_size,
    //         Mat4::IDENTITY,
    //         radius,
    //     );
    //     assert!(results.is_empty());
    // }

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
