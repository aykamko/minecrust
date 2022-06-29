use super::vertex::Vertex;

pub struct Face {
    pub vertex_data: Vec<Vertex>,
    pub index_data: Vec<u16>,
}

impl Face {
    pub fn new() -> Self {
        let vertex_data = [
            // bottom (0, 0, 0)
            Vertex::new([1, 0, 1], [0, 1]),
            Vertex::new([0, 0, 1], [1, 1]),
            Vertex::new([0, 0, 0], [1, 0]),
            Vertex::new([1, 0, 0], [0, 0]),
        ];

        let index_data: &[u16] = &[
            0, 1, 2, 2, 3, 0,
        ];

        Self {
            vertex_data: vertex_data.to_vec(),
            index_data: index_data.to_vec(),
        }
    }
}
