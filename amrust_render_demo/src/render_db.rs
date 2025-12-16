use amrust_render::{RenderData, RenderDatabase, Renderable, gpu_mesh::GpuMesh, instance};

pub struct RenderObject {
    pub renderable: Renderable,
    pub instance: instance::GpuInstance,
    pub gpu_mesh_id: u32,
    pub local_resources: Vec<(u32, u32)>, // (index to local_bind_group, slot_index)
}

pub struct RenderDb {
    // textures: Vec<texture::Texture>,
    meshes: Vec<GpuMesh>,
    objects: Vec<RenderObject>,
    // invisible_objects: HashSet<usize>,
    local_bind_groups: Vec<wgpu::BindGroup>,
}

impl RenderDb {
    pub fn new() -> Self {
        Self {
            // textures: vec![],
            meshes: vec![],
            objects: vec![],
            // invisible_objects: HashSet::new(),
            local_bind_groups: vec![],
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

    pub fn add_mesh(&mut self, mesh: GpuMesh) -> u32 {
        self.meshes.push(mesh);

        (self.meshes.len() - 1) as u32
    }

    pub fn add_object(&mut self, object: RenderObject) -> u32 {
        self.objects.push(object);

        (self.objects.len() - 1) as u32
    }

    pub fn clear_all(&mut self) {
        self.objects.clear();
        self.meshes.clear();
        self.local_bind_groups.clear();
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
}

impl RenderDatabase for RenderDb {
    fn get_renderables<'a>(&'a self) -> impl Iterator<Item = RenderData<'a>> {
        self.objects.iter().map(|r| {
            let gpu_mesh = self.meshes.get(r.gpu_mesh_id as usize).unwrap();
            let local_resources = r
                .local_resources
                .iter()
                .map(|resource| {
                    let bind_group = self.local_bind_groups.get(resource.0 as usize).unwrap();
                    (bind_group, resource.1)
                })
                .collect::<Vec<_>>();

            (r.renderable.clone(), gpu_mesh, &r.instance, local_resources)
        })
    }
}
