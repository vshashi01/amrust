use crate::gpu_mesh::GpuMesh;

pub enum Renderables {
    IndexedMesh(GpuMesh),
    Mesh(GpuMesh),
}

impl Renderables {
    pub fn render(
        &self,
        render_pass: &mut wgpu::RenderPass<'_>,
        instance_len: u32,
        local_bind_groups: &[wgpu::BindGroup],
    ) {
        match self {
            Renderables::IndexedMesh(mesh) => {
                render_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                if let Some(index_buffer) = &mesh.index_buffer {
                    render_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);

                    for (index, slot) in &mesh.local_bind_resources {
                        if let Some(bind_group) = local_bind_groups.get(*index as usize) {
                            render_pass.set_bind_group(*slot, bind_group, &[]);
                        } else {
                            panic!("Local bind group not found for index {index}");
                        }
                    }

                    render_pass.draw_indexed(0..mesh.buffer_length, 0, 0..instance_len);
                } else {
                    panic!("Mesh does not have an index buffer");
                }
            }
            Renderables::Mesh(mesh) => {
                render_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                // render_pass.set_bind_group(1, &basic_diffuse_bind_group, &[]);
                for (index, slot) in &mesh.local_bind_resources {
                    if let Some(bind_group) = local_bind_groups.get(*index as usize) {
                        render_pass.set_bind_group(*slot, bind_group, &[]);
                    } else {
                        panic!("Local bind group not found for index {index}");
                    }
                }

                render_pass.draw(0..mesh.buffer_length, 0..instance_len);
            }
        }
    }
}
