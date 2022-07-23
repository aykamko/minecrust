use glam::Vec3;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pos: [f32; 4],
    tex_coord: [f32; 2],
}

pub struct CuboidCoords {
    pub left: f32,
    pub right: f32,
    pub bottom: f32,
    pub top: f32,
    pub near: f32,
    pub far: f32,
}

pub struct QuadListRenderData {
    pub vertex_data: Vec<Vertex>,
    pub index_data: Vec<u16>,
}

impl Vertex {
    pub fn new(pos: [i8; 3], tc: [i8; 2]) -> Self {
        Self {
            pos: [pos[0] as f32, pos[1] as f32, pos[2] as f32, 1.0],
            tex_coord: [tc[0] as f32, tc[1] as f32],
        }
    }

    pub fn new_from_vec(pos: glam::Vec4) -> Self {
        Self {
            pos: pos.into(),
            tex_coord: [0.0, 0.0],
        }
    }

    pub fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        use std::mem;
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<Vertex>() as wgpu::BufferAddress,
            // We need to switch from using a step mode of Vertex to Instance
            // This means that our shaders will only change to use the next
            // instance when the shader starts processing a new instance
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                // position
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 0,
                    shader_location: 0,
                },
                // tex_coord
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: mem::size_of::<[f32; 4]>() as wgpu::BufferAddress,
                    shader_location: 1,
                },
            ],
        }
    }

    pub fn generate_quad_data(
        quads: &Vec<[glam::Vec3; 4]>,
        maybe_projection: Option<glam::Mat4>,
    ) -> QuadListRenderData {
        let mut vertex_data: Vec<Vertex> = vec![];
        let mut index_data: Vec<u16> = vec![];

        let proj = match maybe_projection {
            Some(_) => maybe_projection.unwrap(),
            None => glam::Mat4::IDENTITY,
        };

        for (i, quad) in quads.iter().enumerate() {
            vertex_data.extend([
                Vertex::new_from_vec(proj * glam::Vec4::new(quad[0].x, quad[0].y, quad[0].z, 1.0)),
                Vertex::new_from_vec(proj * glam::Vec4::new(quad[1].x, quad[1].y, quad[1].z, 1.0)),
                Vertex::new_from_vec(proj * glam::Vec4::new(quad[2].x, quad[2].y, quad[2].z, 1.0)),
                Vertex::new_from_vec(proj * glam::Vec4::new(quad[3].x, quad[3].y, quad[3].z, 1.0)),
            ]);

            let offset = i * 4;
            index_data.extend([0, 1, 2, 2, 3, 0].map(|j| (offset + j) as u16));
        }

        QuadListRenderData {
            vertex_data,
            index_data,
        }
    }

    pub fn generate_quad_data_for_cube(
        cc: &CuboidCoords,
        maybe_projection: Option<glam::Mat4>,
    ) -> QuadListRenderData {
        Vertex::generate_quad_data(
            &vec![
                // left face
                [
                    Vec3::new(cc.left, cc.top, cc.near),
                    Vec3::new(cc.left, cc.top, cc.far),
                    Vec3::new(cc.left, cc.bottom, cc.far),
                    Vec3::new(cc.left, cc.bottom, cc.near),
                ],
                // right face
                [
                    Vec3::new(cc.right, cc.top, cc.far),
                    Vec3::new(cc.right, cc.top, cc.near),
                    Vec3::new(cc.right, cc.bottom, cc.near),
                    Vec3::new(cc.right, cc.bottom, cc.far),
                ],
                // bottom face
                [
                    Vec3::new(cc.left, cc.bottom, cc.near),
                    Vec3::new(cc.left, cc.bottom, cc.far),
                    Vec3::new(cc.right, cc.bottom, cc.far),
                    Vec3::new(cc.right, cc.bottom, cc.near),
                ],
                // top face
                [
                    Vec3::new(cc.right, cc.top, cc.near),
                    Vec3::new(cc.right, cc.top, cc.far),
                    Vec3::new(cc.left, cc.top, cc.far),
                    Vec3::new(cc.left, cc.top, cc.near),
                ],
                // near face
                [
                    Vec3::new(cc.right, cc.top, cc.near),
                    Vec3::new(cc.left, cc.top, cc.near),
                    Vec3::new(cc.left, cc.bottom, cc.near),
                    Vec3::new(cc.right, cc.bottom, cc.near),
                ],
                // far face
                [
                    Vec3::new(cc.right, cc.top, cc.far),
                    Vec3::new(cc.left, cc.top, cc.far),
                    Vec3::new(cc.left, cc.bottom, cc.far),
                    Vec3::new(cc.right, cc.bottom, cc.far),
                ],
            ],
            maybe_projection,
        )
    }
}
