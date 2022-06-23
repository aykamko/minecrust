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
}

pub fn create_vertices() -> (Vec<Vertex>, Vec<u16>) {
    let vertex_data = [
        // front (0, 0, 1)
        Vertex::new([-1, -1, 1], [1, 1], [0, 0]),
        Vertex::new([1, -1, 1], [0, 1], [0, 0]),
        Vertex::new([1, 1, 1], [0, 0], [0, 0]),
        Vertex::new([-1, 1, 1], [1, 0], [0, 0]),
        // back (0, 0, -1)
        Vertex::new([-1, 1, -1], [1, 0], [0, 0]),
        Vertex::new([1, 1, -1], [0, 0], [0, 0]),
        Vertex::new([1, -1, -1], [0, 1], [0, 0]),
        Vertex::new([-1, -1, -1], [1, 1], [0, 0]),
        // right (1, 0, 0)
        Vertex::new([1, -1, -1], [1, 1], [0, 0]),
        Vertex::new([1, 1, -1], [1, 0], [0, 0]),
        Vertex::new([1, 1, 1], [0, 0], [0, 0]),
        Vertex::new([1, -1, 1], [0, 1], [0, 0]),
        // left (-1, 0, 0)
        Vertex::new([-1, -1, 1], [0, 1], [0, 0]),
        Vertex::new([-1, 1, 1], [0, 0], [0, 0]),
        Vertex::new([-1, 1, -1], [1, 0], [0, 0]),
        Vertex::new([-1, -1, -1], [1, 1], [0, 0]),
        // top (0, 1, 0)
        Vertex::new([1, 1, -1], [1, 0], [1, 0]),
        Vertex::new([-1, 1, -1], [0, 0], [1, 0]),
        Vertex::new([-1, 1, 1], [0, 1], [1, 0]),
        Vertex::new([1, 1, 1], [1, 1], [1, 0]),
        // bottom (0, -1, 0)
        Vertex::new([1, -1, 1], [0, 0], [2, 0]),
        Vertex::new([-1, -1, 1], [1, 0], [2, 0]),
        Vertex::new([-1, -1, -1], [1, 1], [2, 0]),
        Vertex::new([1, -1, -1], [0, 1], [2, 0]),
    ];

    let index_data: &[u16] = &[
        0, 1, 2, 2, 3, 0, // front
        4, 5, 6, 6, 7, 4, // back
        8, 9, 10, 10, 11, 8, // right
        12, 13, 14, 14, 15, 12, // left
        16, 17, 18, 18, 19, 16, // top
        20, 21, 22, 22, 23, 20, // bottom
    ];

    (vertex_data.to_vec(), index_data.to_vec())
}
