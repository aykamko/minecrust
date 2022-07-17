#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct InstanceRaw {
    position: [f32; 4],
    rotation: [f32; 4],
    texture_atlas_offset: [f32; 2],
    color_adjust: [f32; 4],
}

impl InstanceRaw {
    pub fn new(
        position: cgmath::Vector3<f32>,
        rotation: cgmath::Quaternion<f32>,
        texture_atlas_offset: [f32; 2],
        color_adjust: [f32; 4],
    ) -> Self {
        InstanceRaw {
            position: [position.x, position.y, position.z, 1.0],
            rotation: [rotation.v.x, rotation.v.y, rotation.v.z, rotation.s],
            texture_atlas_offset: texture_atlas_offset,
            color_adjust: color_adjust,
        }
    }

    pub fn size() -> usize {
        use std::mem;
        mem::size_of::<InstanceRaw>()
    }

    pub fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        use std::mem;
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<InstanceRaw>() as wgpu::BufferAddress,
            // We need to switch from using a step mode of Vertex to Instance
            // This means that our shaders will only change to use the next
            // instance when the shader starts processing a new instance
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32; 4]>() as wgpu::BufferAddress,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32; 8]>() as wgpu::BufferAddress,
                    shader_location: 9,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32; 10]>() as wgpu::BufferAddress,
                    shader_location: 10,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}
