use cgmath::prelude::*;

#[derive(Copy, Clone)]
struct Block {
    // TODO: should be an enum
    block_type: u8,
}

// const WORLD_XZ_SIZE: usize = 846;
// const WORLD_Y_SIZE: usize = 256;
const WORLD_XZ_SIZE: usize = 16;
const WORLD_Y_SIZE: usize = 256;
const BLOCK_SIZE: f32 = 2.0;

impl Default for Block {
    fn default() -> Block {
        Block { block_type: 0 }
    }
}

/*
Jun 23:
- this won't work! How will I quickly find which block to render? I'd have to iterate through this entire array
- even if I start at the eye and render outwards, it would take a really long time
- good for ray tracing though...
- CONCLUSION: whatever, ill deal with it later

 */
pub struct WorldState {
    blocks: [[[Block; WORLD_XZ_SIZE]; WORLD_Y_SIZE]; WORLD_XZ_SIZE],
}

impl WorldState {
    pub fn new() -> Self {
        Self {
            blocks: [[[Block { block_type: 0 }; WORLD_XZ_SIZE]; WORLD_Y_SIZE]; WORLD_XZ_SIZE],
        }
    }

    pub fn initial_setup(&mut self) {
        for (x, z) in iproduct!(0..WORLD_XZ_SIZE, 0..WORLD_XZ_SIZE) {
            self.blocks[x][0][z].block_type = 1; // grass
            self.blocks[x][1][z].block_type = 2; // dirt
        }
    }

    pub fn generate_vertex_data(
        &self,
    ) -> (
        Vec<super::lib::Instance>,
        Vec<super::lib::Instance>,
        Vec<super::lib::InstanceRaw>,
        Vec<super::lib::InstanceRaw>,
    ) {
        let null_rotation =
            cgmath::Quaternion::from_axis_angle(cgmath::Vector3::unit_y(), cgmath::Deg(0.0));
        let mut grass_instances: Vec<super::lib::Instance> = vec![];
        let mut dirt_instances: Vec<super::lib::Instance> = vec![];

        for (x, y, z) in iproduct!(0..WORLD_XZ_SIZE, 0..WORLD_Y_SIZE, 0..WORLD_XZ_SIZE) {
            let position = cgmath::Vector3 {
                x: x as f32 * BLOCK_SIZE,
                y: y as f32 * BLOCK_SIZE,
                z: z as f32 * BLOCK_SIZE,
            };
            match self.blocks[x][y][z].block_type {
                1 => {
                    // println!("grass @ {},{},{}", x, y, z);
                    grass_instances.push(super::lib::Instance {
                        position,
                        rotation: null_rotation,
                    });
                }
                2 => {
                    // println!("dirt @ {},{},{}", x, y, z);
                    dirt_instances.push(super::lib::Instance {
                        position,
                        rotation: null_rotation,
                    });
                }
                _ => {}
            }
        }

        let grass_instance_data = grass_instances
            .iter()
            .map(super::lib::Instance::to_raw)
            .collect::<Vec<_>>();
        let dirt_instance_data = dirt_instances
            .iter()
            .map(super::lib::Instance::to_raw)
            .collect::<Vec<_>>();

        (
            grass_instances,
            dirt_instances,
            grass_instance_data,
            dirt_instance_data,
        )
    }
}
