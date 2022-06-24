#[path = "vertex.rs"]
mod vertex;

use vertex::Vertex;

pub struct Cube {
    pub vertex_data: Vec<Vertex>,
    pub index_data: Vec<u16>,
}

impl Cube {
    fn new(top_atlas_offset: [i8;2], bottom_atlas_offset: [i8;2], side_atlas_offset: [i8;2]) -> Self {
        let (tao, bao, sao) = (top_atlas_offset, bottom_atlas_offset, side_atlas_offset);
        let vertex_data = [
            // front (0, 0, 1)
            Vertex::new([0, 0, 1], [1, 1], sao),
            Vertex::new([1, 0, 1], [0, 1], sao),
            Vertex::new([1, 1, 1], [0, 0], sao),
            Vertex::new([0, 1, 1], [1, 0], sao),
            // back (0, 0, 0)
            Vertex::new([0, 1, 0], [1, 0], sao),
            Vertex::new([1, 1, 0], [0, 0], sao),
            Vertex::new([1, 0, 0], [0, 1], sao),
            Vertex::new([0, 0, 0], [1, 1], sao),
            // right (1, 0, 0)
            Vertex::new([1, 0, 0], [1, 1], sao),
            Vertex::new([1, 1, 0], [1, 0], sao),
            Vertex::new([1, 1, 1], [0, 0], sao),
            Vertex::new([1, 0, 1], [0, 1], sao),
            // left (0, 0, 0)
            Vertex::new([0, 0, 1], [0, 1], sao),
            Vertex::new([0, 1, 1], [0, 0], sao),
            Vertex::new([0, 1, 0], [1, 0], sao),
            Vertex::new([0, 0, 0], [1, 1], sao),
            // top (0, 1, 0)
            Vertex::new([1, 1, 0], [1, 0], tao),
            Vertex::new([0, 1, 0], [0, 0], tao),
            Vertex::new([0, 1, 1], [0, 1], tao),
            Vertex::new([1, 1, 1], [1, 1], tao),
            // bottom (0, 0, 0)
            Vertex::new([1, 0, 1], [0, 0], bao),
            Vertex::new([0, 0, 1], [1, 0], bao),
            Vertex::new([0, 0, 0], [1, 1], bao),
            Vertex::new([1, 0, 0], [0, 1], bao),
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

    pub fn new_grass_block() -> Self {
        Cube::new([1, 0], [2, 0], [0, 0])
    }

    pub fn new_dirt_block() -> Self {
        Cube::new([2, 0], [2, 0], [2, 0])
    }
}