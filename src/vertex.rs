#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Vertex {
    pos: [f32; 4],
    tex_coord: [f32; 2],
    atlas_offset: [f32; 2],
}

impl Vertex {
    pub fn new(pos: [i8; 3], tc: [i8; 2], ao: [i8; 2]) -> Self {
        Self {
            pos: [pos[0] as f32, pos[1] as f32, pos[2] as f32, 1.0],
            tex_coord: [tc[0] as f32, tc[1] as f32],
            atlas_offset: [ao[0] as f32, ao[1] as f32],
        }
    }

    pub fn new_from_pos(pos: [f32; 3]) -> Self {
        Self {
            pos: [pos[0], pos[1], pos[2], 1.0],
            tex_coord: [0.0, 0.0],
            atlas_offset: [0.0, 0.0],
        }
    }
}
