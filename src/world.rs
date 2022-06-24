use cgmath::prelude::*;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

#[derive(Copy, Clone)]
struct Block {
    // TODO: should be an enum
    block_type: u8,
}

// const WORLD_XZ_SIZE: usize = 846;
// const WORLD_Y_SIZE: usize = 256;
const WORLD_XZ_SIZE: usize = 16;
const WORLD_Y_SIZE: usize = 256;
const BLOCK_SIZE: f32 = 1.0;

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
            self.blocks[x][0][z].block_type = 2; // dirt
            self.blocks[x][1][z].block_type = 1; // grass
            println!("Block at {},{},{}", x, 1, z);
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
        let func_start = Instant::now();

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
                    grass_instances.push(super::lib::Instance {
                        position,
                        rotation: null_rotation,
                    });
                }
                2 => {
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

        let elapsed_time = func_start.elapsed().as_millis();
        println!("Took {}ms to generate vertex data", elapsed_time);

        (
            grass_instances,
            dirt_instances,
            grass_instance_data,
            dirt_instance_data,
        )
    }

    pub fn break_block(&mut self, camera: &super::camera::Camera) {
        let mut all_candidate_cubes: Vec<[usize; 3]> = vec![];

        let forward_unit = (camera.target - camera.eye).normalize();

        println!("Camera eye is at {:?}", camera.eye / BLOCK_SIZE);

        let mut curr_pos = camera.eye;
        curr_pos -= forward_unit;

        const MAX_ITER: usize = 20 + 1;
        for _ in 0..MAX_ITER {
            curr_pos += forward_unit;
            let cube = [
                (curr_pos.x / BLOCK_SIZE).floor() as usize,
                (curr_pos.y / BLOCK_SIZE).floor() as usize,
                (curr_pos.z / BLOCK_SIZE).floor() as usize,
            ];
            println!("Adding cube {:?}", cube);
            all_candidate_cubes.push(cube);
            // TODO: add neighboring cubes too
        }

        for cube in all_candidate_cubes.iter() {
            let val = self.blocks[cube[0]][cube[1]][cube[2]].block_type;
            println!("Checking cube {:?}: {}", cube, val);
            if self.blocks[cube[0]][cube[1]][cube[2]].block_type != 0 {
                self.blocks[cube[0]][cube[1]][cube[2]].block_type = 0;
                break;
            }
        }
    }
    /*
    # Ray intersection algo v2:

    start at eye e
    all_candidate_cubes = []
    repeat for N steps  # N = 20ish
      add unit vector in direction t  # t = target
      candidate_cubes_this_iter = []
      for all possible intersecting cubes  # possible intersection means we added +1 to the axis
        if cube exists in world
          add cube to candidate_cubes_this_iter
      all_candidate_cubes.extend(candidate_cubes_this_iter)
      if candidate_cubes_this_iter === 7:  # optimization: we had to have hit something here
        break
    for cube in all_candidate_cubes:
      check intersection using ray-tracing-lin-alg  # https://www.scratchapixel.com/lessons/3d-basic-rendering/minimal-ray-tracer-rendering-simple-shapes/ray-box-intersection
     */
}
