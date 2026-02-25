use amrust_render::{
    RenderData3d, RenderDataLocalResources, Renderable3d,
    gpu_mesh::GpuMesh,
    instance::{self, GpuInstance},
};
use rkyv::collections::btree_map::Range;
use slotmap::{SlotMap, new_key_type};

pub struct RenderObject {
    pub renderable: Renderable3d,
    pub instance: instance::GpuInstance,
    pub gpu_mesh_id: RenderMeshId,
    pub mesh_local: RenderDataLocalResources,
}

pub struct RenderObjectNew {
    pub renderable: Renderable3d,
    pub mesh: RenderMeshId,
    pub instance: RenderInstanceId,
    pub local_render_data: RenderDataLocalResourcesId,
    pub mesh_range: Option<std::ops::Range<u32>>,
    pub instance_range: Option<std::ops::Range<u32>>,
}

new_key_type! {
    pub struct RenderMeshId;
}

new_key_type! {
    pub struct RenderObjectId;
}

new_key_type! {
    pub struct RenderInstanceId;
}

new_key_type! {
    pub struct RenderDataLocalResourcesId;
}

pub struct RenderDb {
    // textures: Vec<texture::Texture>,
    meshes: SlotMap<RenderMeshId, GpuMesh>,
    instances: SlotMap<RenderInstanceId, GpuInstance>,
    local_render_data: SlotMap<RenderDataLocalResourcesId, RenderDataLocalResources>,
    objects: SlotMap<RenderObjectId, RenderObject>,
    // invisible_objects: HashSet<usize>,
    objects_to_render: Vec<RenderObjectId>,
    objects_to_render_new: Vec<RenderObjectNew>,
}

impl RenderDb {
    pub fn new() -> Self {
        Self {
            // textures: vec![],
            meshes: SlotMap::with_key(),
            instances: SlotMap::with_key(),
            local_render_data: SlotMap::with_key(),
            objects: SlotMap::with_key(),
            // invisible_objects: HashSet::new(),
            objects_to_render: vec![],
            objects_to_render_new: vec![],
        }
    }

    pub fn set_objects_to_render(&mut self, objects: &[RenderObjectId]) {
        self.objects_to_render.clear();
        for o in objects {
            self.objects_to_render.push(*o);
        }
    }

    // returns the texture id and the bind group id
    // pub fn add_texture(
    //     &mut self,
    //     texture: texture::Texture,
    //     device: &wgpu::Device,
    //     sampler: &wgpu::Sampler,
    //     layout: &wgpu::BindGroupLayout,
    // ) -> (u32, u32) {
    //     let bind_group =
    //         texture::generate_basic_texture_bind_group::<0, 1>(device, &texture, sampler, layout);

    //     self.textures.push(texture);
    //     let bind_group_id = self.add_local_bind_group(bind_group);
    //     let texture_id = (self.textures.len() - 1) as u32;

    //     (texture_id, bind_group_id)
    // }

    // returns the bind group id for the texture array
    // pub fn create_texture_array(
    //     &mut self,
    //     texture_ids: &[u32],
    //     device: &wgpu::Device,
    //     sampler: &wgpu::Sampler,
    //     layout: &wgpu::BindGroupLayout,
    // ) -> u32 {
    //     let mut texture_views = Vec::<&wgpu::TextureView>::new();

    //     for texture_id in texture_ids {
    //         let texture = &self.textures[*texture_id as usize];
    //         texture_views.push(&texture.view);
    //     }

    //     let bind_group = texture::generate_texture_array_bind_group::<0, 1>(
    //         device,
    //         "Array 1",
    //         &texture_views,
    //         sampler,
    //         layout,
    //     );

    //     self.add_local_bind_group(bind_group)
    // }

    // pub fn render_mesh_exist(&self, ids: &[RenderMeshId]) -> bool {
    //     ids.iter().all(|id| self.meshes.contains_key(*id))
    // }

    // pub fn render_object_exist(&self, ids: &[RenderObjectId]) -> bool {
    //     ids.iter().all(|id| self.objects.contains_key(*id))
    // }

    pub fn add_mesh(&mut self, mesh: GpuMesh) -> RenderMeshId {
        self.meshes.insert(mesh)
    }

    pub fn add_instance(&mut self, instance: GpuInstance) -> RenderInstanceId {
        self.instances.insert(instance)
    }

    pub fn add_local_render_resource(
        &mut self,
        resource: RenderDataLocalResources,
    ) -> RenderDataLocalResourcesId {
        self.local_render_data.insert(resource)
    }

    pub fn add_object(&mut self, object: RenderObject) -> RenderObjectId {
        self.objects.insert(object)
    }

    pub fn remove_object(&mut self, id: RenderObjectId) -> Option<RenderObject> {
        self.objects.remove(id)
    }

    pub fn add_render_objects_new(&mut self, objects: Vec<RenderObjectNew>) {
        self.objects_to_render_new = objects;
    }

    // pub fn clear_objects_to_render(&mut self) {
    //     self.objects_to_render.clear();
    // }

    pub fn clear_all(&mut self) {
        self.objects.clear();
        self.meshes.clear();
        self.objects_to_render.clear();
    }

    // fn make_object_invisible(&mut self, object_id: usize) {
    //     self.invisible_objects.insert(object_id);
    // }

    // fn make_object_visible(&mut self, object_id: &usize) {
    //     self.invisible_objects.remove(object_id);
    // }

    // pub fn add_local_bind_group(&mut self, bind_group: wgpu::BindGroup) -> u32 {
    //     self.local_bind_groups.push(bind_group);

    //     (self.local_bind_groups.len() - 1) as u32
    // }

    pub fn get_renderables<'a>(&'a self) -> impl Iterator<Item = RenderData3d<'a>> {
        self.objects.iter().filter_map(|(id, render_object)| {
            if self.objects_to_render.contains(&id) {
                let gpu_mesh = self.meshes.get(render_object.gpu_mesh_id).unwrap();

                Some(RenderData3d {
                    renderable: render_object.renderable.clone(),
                    mesh: gpu_mesh,
                    instance: &render_object.instance,
                    local_resources: &render_object.mesh_local,
                    triangle_face_mode: None,
                    back_material: None,
                    mesh_element_range: None,
                    instance_range: None,
                })
            } else {
                None
            }
        })
    }
}
