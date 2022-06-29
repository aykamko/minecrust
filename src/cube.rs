use super::vertex::Vertex;

pub struct Cube {
    pub vertex_data: Vec<Vertex>,
    pub index_data: Vec<u16>,
}

impl Cube {
    pub fn new() -> Self {
        let vertex_data = [
            // front (0, 0, 1)
            Vertex::new([0, 0, 1], [1, 1]),
            Vertex::new([1, 0, 1], [0, 1]),
            Vertex::new([1, 1, 1], [0, 0]),
            Vertex::new([0, 1, 1], [1, 0]),
            // back (0, 0, 0)
            Vertex::new([0, 1, 0], [1, 0]),
            Vertex::new([1, 1, 0], [0, 0]),
            Vertex::new([1, 0, 0], [0, 1]),
            Vertex::new([0, 0, 0], [1, 1]),
            // right (1, 0, 0)
            Vertex::new([1, 0, 0], [1, 1]),
            Vertex::new([1, 1, 0], [1, 0]),
            Vertex::new([1, 1, 1], [0, 0]),
            Vertex::new([1, 0, 1], [0, 1]),
            // left (0, 0, 0)
            Vertex::new([0, 0, 1], [0, 1]),
            Vertex::new([0, 1, 1], [0, 0]),
            Vertex::new([0, 1, 0], [1, 0]),
            Vertex::new([0, 0, 0], [1, 1]),
            // top (0, 1, 0)
            Vertex::new([1, 1, 0], [1, 0]),
            Vertex::new([0, 1, 0], [0, 0]),
            Vertex::new([0, 1, 1], [0, 1]),
            Vertex::new([1, 1, 1], [1, 1]),
            // bottom (0, 0, 0)
            Vertex::new([1, 0, 1], [0, 0]),
            Vertex::new([0, 0, 1], [1, 0]),
            Vertex::new([0, 0, 0], [1, 1]),
            Vertex::new([1, 0, 0], [0, 1]),
        ];

        let index_data: &[u16] = &[
            0, 1, 2, 2, 3, 0, // front
            4, 5, 6, 6, 7, 4, // back
            8, 9, 10, 10, 11, 8, // right
            12, 13, 14, 14, 15, 12, // left
            16, 17, 18, 18, 19, 16, // top
            20, 21, 22, 22, 23, 20, // bottom
        ];

        Self {
            vertex_data: vertex_data.to_vec(),
            index_data: index_data.to_vec(),
        }
    }

    pub fn grass_atlas_offsets() -> [[f32; 2]; 3] {
        [[1.0, 0.0], [2.0, 0.0], [0.0, 0.0]]
    }

    pub fn dirt_atlas_offsets() -> [[f32; 2]; 3] {
        [[2.0, 0.0], [2.0, 0.0], [2.0, 0.0]]
    }
}
