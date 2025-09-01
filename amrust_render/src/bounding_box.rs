use core::f32;
use std::fmt::Debug;

use glam::Vec3;

use crate::transformation::Transformation;

#[derive(Clone, Copy)]
pub struct BoundingBox {
    pub min: Vec3,
    pub max: Vec3,
}

impl Default for BoundingBox {
    fn default() -> Self {
        BoundingBox {
            min: Vec3::splat(f32::INFINITY),
            max: Vec3::splat(f32::NEG_INFINITY),
        }
    }
}

impl Debug for BoundingBox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BoundingBox")
            .field("min", &self.min)
            .field("max", &self.max)
            .field("delta", &self.delta())
            .finish()
    }
}

impl BoundingBox {
    pub fn center(&self) -> Vec3 {
        self.min + (self.max - self.min) * 0.5
    }
    pub fn transform(&mut self, transform: &Transformation) {
        self.min = transform.0.transform_point3(self.min);
        self.max = transform.0.transform_point3(self.max);
    }

    pub fn unite(&mut self, other: &BoundingBox) {
        self.min = self.min.min(other.min);
        self.max = self.max.max(other.max);
    }

    pub fn corners(&self) -> [Vec3; 8] {
        [
            Vec3::new(self.min.x, self.min.y, self.min.z),
            Vec3::new(self.max.x, self.min.y, self.min.z),
            Vec3::new(self.min.x, self.max.y, self.min.z),
            Vec3::new(self.max.x, self.max.y, self.min.z),
            Vec3::new(self.min.x, self.min.y, self.max.z),
            Vec3::new(self.max.x, self.min.y, self.max.z),
            Vec3::new(self.min.x, self.max.y, self.max.z),
            Vec3::new(self.max.x, self.max.y, self.max.z),
        ]
    }

    pub fn wireframe_indices() -> [u32; 24] {
        // Each pair is a line segment between two corners
        let box_wireframe_indices: [u32; 24] = [
            // Bottom face (min z)
            0, 1, 1, 3, 3, 2, 2, 0, // Top face (max z)
            4, 5, 5, 7, 7, 6, 6, 4, // Vertical edges
            0, 4, 1, 5, 2, 6, 3, 7,
        ];

        box_wireframe_indices
    }

    pub fn delta(&self) -> Vec3 {
        self.max - self.min
    }
}
