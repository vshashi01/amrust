use crate::core::types::transformation::Transformation;
use crate::models::view_modes::{LightingMode, MaterialOpacity, ShadingMode};
use std::collections::HashMap;
use std::hash::Hash;

use amrust_render::{
    material::{Material, RgbMaterialData},
    transformation::TransformationData,
};

/// Bucket classification for rendering
///
/// Ordered: Opaque (0-5) → Transparent (6-11) → Hidden (separate)
/// This ordering ensures contiguous memory layout in GPU buffers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum RenderBucket {
    OpaqueLitShade = 0,
    OpaqueLitShadeAndWire = 1,
    OpaqueLitWireOnly = 2,
    OpaqueUnlitShade = 3,
    OpaqueUnlitShadeAndWire = 4,
    OpaqueUnlitWireOnly = 5,
    TransparentLitShade = 6,
    TransparentLitShadeAndWire = 7,
    TransparentLitWireOnly = 8,
    TransparentUnlitShade = 9,
    TransparentUnlitShadeAndWire = 10,
    TransparentUnlitWireOnly = 11,
}

impl RenderBucket {
    /// Get array index (0-11)
    pub fn index(self) -> usize {
        self as usize
    }

    /// Check if this is a transparent bucket
    pub fn is_transparent(self) -> bool {
        self.index() >= 6
    }

    /// Check if this bucket needs colored mesh rendering
    pub fn needs_colored(self) -> bool {
        matches!(
            self,
            RenderBucket::OpaqueLitShade
                | RenderBucket::OpaqueLitShadeAndWire
                | RenderBucket::OpaqueUnlitShade
                | RenderBucket::OpaqueUnlitShadeAndWire
                | RenderBucket::TransparentLitShade
                | RenderBucket::TransparentLitShadeAndWire
                | RenderBucket::TransparentUnlitShade
                | RenderBucket::TransparentUnlitShadeAndWire
        )
    }

    /// Check if this bucket needs wireframe rendering
    pub fn needs_wireframe(self) -> bool {
        matches!(
            self,
            RenderBucket::OpaqueLitWireOnly
                | RenderBucket::OpaqueLitShadeAndWire
                | RenderBucket::OpaqueUnlitWireOnly
                | RenderBucket::OpaqueUnlitShadeAndWire
                | RenderBucket::TransparentLitWireOnly
                | RenderBucket::TransparentLitShadeAndWire
                | RenderBucket::TransparentUnlitWireOnly
                | RenderBucket::TransparentUnlitShadeAndWire
        )
    }
}

/// Instance data stored within a bucket
#[derive(Debug, Clone)]
struct InstanceData<T: PartialEq + Clone> {
    pub id: T,
    pub transform: Transformation,
    pub visibility: bool,
    pub shading: ShadingMode,
    pub lighting: LightingMode,
    pub opacity: MaterialOpacity,
    /// Fixed position in GPU buffer - never changes
    buffer_index: u32,
}

/// Location of an instance for O(1) lookup
#[derive(Debug, Clone, Copy)]
struct InstanceLocation {
    /// Which bucket (0-11) or hidden
    pub bucket_index: usize,
    /// Position within the bucket's vector
    pub index_in_bucket: usize,
    /// Global position in GPU buffer
    pub buffer_index: u32,
    /// Whether instance is hidden
    pub is_hidden: bool,
}

/// Main storage for bucketed instance data
///
/// Maintains stable GPU buffer with batched update support.
/// All instances (including hidden) are stored in the GPU buffer
/// for stable instance count, but render ranges exclude hidden instances.
#[derive(Debug)]
pub struct OrderedRenderData<T: PartialEq + Clone + Hash> {
    /// Associated mesh
    // pub gpu_mesh_id: RenderMeshId,

    /// Total instance count (visible + hidden)
    total_instance_count: usize,

    /// 12 buckets for visible instances (indexed by RenderBucket)
    buckets: [Vec<InstanceData<T>>; 12],

    /// Hidden instances - still in GPU buffer but not rendered
    hidden: Vec<InstanceData<T>>,

    /// Fast lookup: PartInstanceId -> location
    instance_lookup: HashMap<T, InstanceLocation>,

    /// Flag indicating data needs rebuild
    needs_rebucket: bool,
}

impl<T: PartialEq + Eq + Clone + Hash> OrderedRenderData<T> {
    /// Create new empty bucketed data for a mesh
    pub fn new() -> Self {
        Self {
            // gpu_mesh_id,
            total_instance_count: 0,
            buckets: [
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
            ],
            hidden: Vec::new(),
            instance_lookup: HashMap::new(),
            // render_instance_buffer_id: None,
            needs_rebucket: false,
        }
    }

    /// Classify instance properties into a bucket
    pub fn classify_bucket(
        opacity: MaterialOpacity,
        lighting: LightingMode,
        shading: ShadingMode,
    ) -> RenderBucket {
        match (opacity, lighting, shading) {
            (MaterialOpacity::Opaque, LightingMode::Lit, ShadingMode::Shade) => {
                RenderBucket::OpaqueLitShade
            }
            (MaterialOpacity::Opaque, LightingMode::Lit, ShadingMode::ShadeAndWire) => {
                RenderBucket::OpaqueLitShadeAndWire
            }
            (MaterialOpacity::Opaque, LightingMode::Lit, ShadingMode::WireOnly) => {
                RenderBucket::OpaqueLitWireOnly
            }
            (MaterialOpacity::Opaque, LightingMode::Unlit, ShadingMode::Shade) => {
                RenderBucket::OpaqueUnlitShade
            }
            (MaterialOpacity::Opaque, LightingMode::Unlit, ShadingMode::ShadeAndWire) => {
                RenderBucket::OpaqueUnlitShadeAndWire
            }
            (MaterialOpacity::Opaque, LightingMode::Unlit, ShadingMode::WireOnly) => {
                RenderBucket::OpaqueUnlitWireOnly
            }
            (MaterialOpacity::Transparent, LightingMode::Lit, ShadingMode::Shade) => {
                RenderBucket::TransparentLitShade
            }
            (MaterialOpacity::Transparent, LightingMode::Lit, ShadingMode::ShadeAndWire) => {
                RenderBucket::TransparentLitShadeAndWire
            }
            (MaterialOpacity::Transparent, LightingMode::Lit, ShadingMode::WireOnly) => {
                RenderBucket::TransparentLitWireOnly
            }
            (MaterialOpacity::Transparent, LightingMode::Unlit, ShadingMode::Shade) => {
                RenderBucket::TransparentUnlitShade
            }
            (MaterialOpacity::Transparent, LightingMode::Unlit, ShadingMode::ShadeAndWire) => {
                RenderBucket::TransparentUnlitShadeAndWire
            }
            (MaterialOpacity::Transparent, LightingMode::Unlit, ShadingMode::WireOnly) => {
                RenderBucket::TransparentUnlitWireOnly
            }
        }
    }

    /// Insert a new instance at the correct bucket
    ///
    /// This is called during scene traversal. The instance is immediately
    /// classified and placed in the appropriate bucket, with a fixed buffer
    /// index assigned sequentially.
    pub fn insert_instance(
        &mut self,
        id: T,
        transform: Transformation,
        visibility: bool,
        opacity: MaterialOpacity,
        lighting: LightingMode,
        shading: ShadingMode,
    ) {
        let buffer_index = self.total_instance_count as u32;

        if !visibility {
            // Hidden - store in hidden vec but still assign buffer index
            let index_in_bucket = self.hidden.len();
            self.hidden.push(InstanceData {
                id: id.clone(),
                transform,
                visibility: false,
                shading,
                lighting,
                opacity,
                buffer_index,
            });

            self.instance_lookup.insert(
                id,
                InstanceLocation {
                    bucket_index: 0, // Not used for hidden
                    index_in_bucket,
                    buffer_index,
                    is_hidden: true,
                },
            );
        } else {
            // Visible - classify and store in appropriate bucket
            let bucket = Self::classify_bucket(opacity, lighting, shading);
            let bucket_idx = bucket.index();
            let index_in_bucket = self.buckets[bucket_idx].len();

            self.buckets[bucket_idx].push(InstanceData {
                id: id.clone(),
                transform,
                visibility: true,
                shading,
                lighting,
                opacity,
                buffer_index,
            });

            self.instance_lookup.insert(
                id,
                InstanceLocation {
                    bucket_index: bucket_idx,
                    index_in_bucket,
                    buffer_index,
                    is_hidden: false,
                },
            );
        }

        self.total_instance_count += 1;
        self.needs_rebucket = true;
    }

    pub fn rebucket(&mut self) {
        if !self.needs_rebucket {
            return;
        }

        // Collect all instances from all buckets and hidden
        let mut all_instances = Vec::with_capacity(self.total_instance_count);

        for bucket in &self.buckets {
            all_instances.extend(bucket.iter().cloned());
        }
        all_instances.extend(self.hidden.iter().cloned());

        // Clear all buckets
        for bucket in &mut self.buckets {
            bucket.clear();
        }
        self.hidden.clear();
        self.instance_lookup.clear();
        self.total_instance_count = 0;

        // Re-insert all instances with fresh classification
        for mut instance in all_instances {
            let new_buffer_index = self.total_instance_count as u32;
            instance.buffer_index = new_buffer_index;

            if !instance.visibility {
                // Hidden instance
                let index_in_bucket = self.hidden.len();
                self.hidden.push(instance.clone());

                self.instance_lookup.insert(
                    instance.id,
                    InstanceLocation {
                        bucket_index: 0,
                        index_in_bucket,
                        buffer_index: new_buffer_index,
                        is_hidden: true,
                    },
                );
            } else {
                // Visible - classify into appropriate bucket
                let bucket =
                    Self::classify_bucket(instance.opacity, instance.lighting, instance.shading);
                let bucket_idx = bucket.index();
                let index_in_bucket = self.buckets[bucket_idx].len();

                self.buckets[bucket_idx].push(instance.clone());

                self.instance_lookup.insert(
                    instance.id,
                    InstanceLocation {
                        bucket_index: bucket_idx,
                        index_in_bucket,
                        buffer_index: new_buffer_index,
                        is_hidden: false,
                    },
                );
            }

            self.total_instance_count += 1;
        }

        self.needs_rebucket = false;
    }

    /// Build ordered data vectors for GPU buffer
    ///
    /// Order: Buckets 0-11 (visible), then hidden instances
    pub fn build_data_vectors(&self) -> (Vec<TransformationData>, Vec<RgbMaterialData>) {
        let mut transforms = Vec::with_capacity(self.total_instance_count);
        let mut materials = Vec::with_capacity(self.total_instance_count);

        // Build in bucket order
        for bucket in &self.buckets {
            for instance in bucket {
                transforms.push(instance_to_transformation_data(&instance.transform));
                materials.push(Material::new(1.0, 1.0, 1.0).to_data());
            }
        }

        // Hidden instances at the end (stable count)
        for instance in &self.hidden {
            transforms.push(instance_to_transformation_data(&instance.transform));
            materials.push(Material::new(1.0, 1.0, 1.0).to_data());
        }

        (transforms, materials)
    }

    /// Calculate render ranges for each bucket
    ///
    /// Returns map of bucket -> instance range in GPU buffer.
    /// Hidden instances are not included in any range.
    pub fn calculate_render_ranges(&self) -> HashMap<RenderBucket, std::ops::Range<u32>> {
        let mut ranges = HashMap::new();
        let mut current_index = 0u32;

        for (bucket_idx, bucket) in self.buckets.iter().enumerate() {
            let count = bucket.len() as u32;
            if count > 0 {
                let bucket_type: RenderBucket = unsafe { std::mem::transmute(bucket_idx as u8) };
                ranges.insert(bucket_type, current_index..current_index + count);
                current_index += count;
            }
        }
        // Hidden instances not included - they're at the end but not rendered

        ranges
    }

    /// Update instance transform
    pub fn update_transform(&mut self, id: T, transform: Transformation) -> bool {
        if let Some(loc) = self.instance_lookup.get(&id) {
            let instance = if loc.is_hidden {
                &mut self.hidden[loc.index_in_bucket]
            } else {
                &mut self.buckets[loc.bucket_index][loc.index_in_bucket]
            };
            instance.transform = transform;
            self.needs_rebucket = true;
            true
        } else {
            false
        }
    }

    pub fn update_visibility(&mut self, id: T, visible: bool) -> bool {
        if let Some(loc) = self.instance_lookup.get(&id) {
            let instance = if loc.is_hidden {
                &mut self.hidden[loc.index_in_bucket]
            } else {
                &mut self.buckets[loc.bucket_index][loc.index_in_bucket]
            };

            if instance.visibility != visible {
                instance.visibility = visible;
                self.needs_rebucket = true;
            }
            true
        } else {
            false
        }
    }

    pub fn update_shading(
        &mut self,
        id: T,
        opacity: MaterialOpacity,
        shading: ShadingMode,
        lighting: LightingMode,
    ) -> bool {
        if let Some(loc) = self.instance_lookup.get(&id) {
            let instance = if loc.is_hidden {
                &mut self.hidden[loc.index_in_bucket]
            } else {
                &mut self.buckets[loc.bucket_index][loc.index_in_bucket]
            };

            let changed = instance.opacity != opacity
                || instance.shading != shading
                || instance.lighting != lighting;

            if changed {
                instance.opacity = opacity;
                instance.shading = shading;
                instance.lighting = lighting;
                self.needs_rebucket = true;
            }
            true
        } else {
            false
        }
    }

    /// Get total visible instance count
    pub fn visible_count(&self) -> usize {
        self.total_instance_count - self.hidden.len()
    }

    /// Get total instance count (visible + hidden)
    pub fn total_count(&self) -> usize {
        self.total_instance_count
    }
}

/// Helper: Convert Transformation to GPU format
fn instance_to_transformation_data(transformation: &Transformation) -> TransformationData {
    TransformationData(transformation.0.to_cols_array_2d())
}

#[cfg(test)]
mod tests {
    use crate::core::types::part_instance::PartInstanceId;

    use super::*;

    #[test]
    fn test_bucket_classification() {
        assert_eq!(
            OrderedRenderData::<PartInstanceId>::classify_bucket(
                MaterialOpacity::Opaque,
                LightingMode::Lit,
                ShadingMode::Shade
            ),
            RenderBucket::OpaqueLitShade
        );

        assert_eq!(
            OrderedRenderData::<PartInstanceId>::classify_bucket(
                MaterialOpacity::Transparent,
                LightingMode::Unlit,
                ShadingMode::WireOnly
            ),
            RenderBucket::TransparentUnlitWireOnly
        );
    }

    #[test]
    fn test_insertion_and_ordering() {
        use slotmap::Key as _;

        let mut data = OrderedRenderData::new();

        // Insert instances in arbitrary order
        data.insert_instance(
            PartInstanceId::null(),
            Transformation(glam::Mat4::IDENTITY),
            true,
            MaterialOpacity::Transparent,
            LightingMode::Lit,
            ShadingMode::Shade,
        );

        data.insert_instance(
            PartInstanceId::null(),
            Transformation(glam::Mat4::IDENTITY),
            true,
            MaterialOpacity::Opaque,
            LightingMode::Lit,
            ShadingMode::Shade,
        );

        // Check bucket assignment
        assert_eq!(data.buckets[0].len(), 1); // OpaqueLitShade
        assert_eq!(data.buckets[6].len(), 1); // TransparentLitShade

        // Check total count
        assert_eq!(data.total_count(), 2);
    }

    #[test]
    fn test_update_visibility() {
        use slotmap::Key as _;

        let mut data = OrderedRenderData::new();

        // Insert a visible instance
        let id1 = PartInstanceId::null();
        data.insert_instance(
            id1,
            Transformation(glam::Mat4::IDENTITY),
            true,
            MaterialOpacity::Opaque,
            LightingMode::Lit,
            ShadingMode::Shade,
        );

        assert_eq!(data.buckets[0].len(), 1);
        assert_eq!(data.hidden.len(), 0);
        assert!(data.needs_rebucket);

        // Update to hidden
        data.rebucket();
        assert!(!data.needs_rebucket);

        let result = data.update_visibility(id1, false);
        assert!(result);
        assert!(data.needs_rebucket);

        // After rebucket, should be in hidden
        data.rebucket();
        assert_eq!(data.buckets[0].len(), 0);
        assert_eq!(data.hidden.len(), 1);
        assert!(!data.needs_rebucket);

        // Update back to visible
        let result = data.update_visibility(id1, true);
        assert!(result);
        assert!(data.needs_rebucket);

        data.rebucket();
        assert_eq!(data.buckets[0].len(), 1);
        assert_eq!(data.hidden.len(), 0);
    }

    #[test]
    fn test_update_shading() {
        use slotmap::Key as _;

        let mut data = OrderedRenderData::new();

        // Insert instance in OpaqueLitShade bucket
        let id1 = PartInstanceId::null();
        data.insert_instance(
            id1,
            Transformation(glam::Mat4::IDENTITY),
            true,
            MaterialOpacity::Opaque,
            LightingMode::Lit,
            ShadingMode::Shade,
        );

        assert_eq!(data.buckets[0].len(), 1); // OpaqueLitShade
        data.rebucket();
        assert!(!data.needs_rebucket);

        // Update to different shading (parameter order: opacity, shading, lighting)
        let result = data.update_shading(
            id1,
            MaterialOpacity::Opaque,
            ShadingMode::WireOnly,
            LightingMode::Lit,
        );
        assert!(result);
        assert!(data.needs_rebucket);

        // After rebucket, should be in OpaqueLitWireOnly bucket
        data.rebucket();
        assert_eq!(data.buckets[0].len(), 0); // OpaqueLitShade is empty
        assert_eq!(data.buckets[2].len(), 1); // OpaqueLitWireOnly has 1
    }

    #[test]
    fn test_rebucket_preserves_count() {
        use slotmap::Key as _;

        let mut data = OrderedRenderData::new();

        // Insert 3 visible instances (each with unique ID via null())
        let id1 = PartInstanceId::null();
        data.insert_instance(
            id1,
            Transformation(glam::Mat4::IDENTITY),
            true,
            MaterialOpacity::Opaque,
            LightingMode::Lit,
            ShadingMode::Shade,
        );

        let id2 = PartInstanceId::null();
        data.insert_instance(
            id2,
            Transformation(glam::Mat4::IDENTITY),
            true,
            MaterialOpacity::Opaque,
            LightingMode::Lit,
            ShadingMode::Shade,
        );

        let id3 = PartInstanceId::null();
        data.insert_instance(
            id3,
            Transformation(glam::Mat4::IDENTITY),
            true,
            MaterialOpacity::Opaque,
            LightingMode::Lit,
            ShadingMode::Shade,
        );

        // Note: Due to slotmap::Key::null() returning same value,
        // only the last inserted instance is trackable. This is a limitation
        // of using null() for testing. In production, real unique IDs would be used.

        assert_eq!(data.total_count(), 3);
        assert_eq!(data.buckets[0].len(), 3);

        // Rebucket to organize instances properly
        data.rebucket();
        assert_eq!(data.total_count(), 3);
        assert_eq!(data.buckets[0].len(), 3);

        // Update the last inserted instance (the only one we can track)
        data.update_shading(
            id3,
            MaterialOpacity::Transparent,
            ShadingMode::WireOnly,
            LightingMode::Unlit,
        );

        data.rebucket();
        assert_eq!(data.total_count(), 3);
        // Only the trackable instance moved
        assert_eq!(data.buckets[0].len(), 2); // 2 untrackable instances stay here
        assert_eq!(data.buckets[11].len(), 1); // Trackable instance moved
    }

    #[test]
    fn test_update_nonexistent_instance() {
        use slotmap::Key as _;

        let mut data = OrderedRenderData::new();

        let fake_id = PartInstanceId::null();

        assert!(!data.update_visibility(fake_id, false));
        assert!(!data.update_shading(
            fake_id,
            MaterialOpacity::Opaque,
            ShadingMode::Shade,
            LightingMode::Lit,
        ));
        assert!(!data.update_transform(fake_id, Transformation(glam::Mat4::IDENTITY)));
    }
}
