use crate::cube::Cube;
use bitmaps::Bitmap;

use super::instance::{Instance, InstanceRaw};
use cgmath::prelude::*;
use cgmath_17::MetricSpace;
use collision::{Continuous};
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

#[derive(Copy, Clone)]
struct Block {
    // TODO: should be an enum
    block_type: u8,
    neighbors: Bitmap<8>, // top, bottom, left, right, front, back
}

const WORLD_XZ_SIZE: usize = 128;
const WORLD_Y_SIZE: usize = 256;

impl Default for Block {
    fn default() -> Block {
        Block {
            block_type: 0,
            neighbors: Bitmap::new(),
        }
    }
}

pub struct WorldState {
    blocks: Vec<Block>,
}

impl WorldState {
    pub fn new() -> Self {
        Self {
            blocks: vec![
                Block {
                    ..Default::default()
                };
                WORLD_XZ_SIZE * WORLD_Y_SIZE * WORLD_XZ_SIZE
            ],
        }
    }

    fn index(x: usize, y: usize, z: usize) -> usize {
        x + (y * WORLD_XZ_SIZE) + (z * WORLD_XZ_SIZE * WORLD_Y_SIZE)
    }

    fn set_block(&mut self, x: usize, y: usize, z: usize, block_type: u8) {
        let block = &mut self.blocks[WorldState::index(x, y, z)];
        block.block_type = block_type;

        if y != WORLD_Y_SIZE - 1 {
            let top_neighbor = &mut self.blocks[WorldState::index(x, y + 1, z)];
            top_neighbor.neighbors.set(1, block_type != 0);
        }

        if y != 0 {
            let bottom_neighbor = &mut self.blocks[WorldState::index(x, y - 1, z)];
            bottom_neighbor.neighbors.set(0, block_type != 0);
        }

        if x != WORLD_XZ_SIZE - 1 {
            let left_neighbor = &mut self.blocks[WorldState::index(x + 1, y, z)];
            left_neighbor.neighbors.set(3, block_type != 0);
        }

        if x != 0 {
            let right_neighbor = &mut self.blocks[WorldState::index(x - 1, y, z)];
            right_neighbor.neighbors.set(2, block_type != 0);
        }

        if z != WORLD_XZ_SIZE - 1 {
            let front_neighbor = &mut self.blocks[WorldState::index(x, y, z + 1)];
            front_neighbor.neighbors.set(5, block_type != 0);
        }

        if z != 0 {
            let back_neighbor = &mut self.blocks[WorldState::index(x, y, z - 1)];
            back_neighbor.neighbors.set(4, block_type != 0);
        }
    }

    fn block_at(&self, x: usize, y: usize, z: usize) -> &Block {
        &self.blocks[WorldState::index(x, y, z)]
    }

    pub fn initial_setup(&mut self) {
        for (x, z) in iproduct!(0..WORLD_XZ_SIZE, 0..WORLD_XZ_SIZE) {
            self.set_block(x, 0, z, 2); // dirt
            self.set_block(x, 1, z, 1); // grass
        }
    }

    pub fn generate_vertex_data(&self) -> (Vec<Instance>, Vec<InstanceRaw>) {
        let func_start = Instant::now();

        let null_rotation =
            cgmath::Quaternion::from_axis_angle(cgmath::Vector3::unit_y(), cgmath::Deg(0.0));
        let mut instances: Vec<Instance> = vec![];

        for (x, y, z) in iproduct!(0..WORLD_XZ_SIZE, 0..WORLD_Y_SIZE, 0..WORLD_XZ_SIZE) {
            let position = cgmath::Vector3 {
                x: x as f32,
                y: y as f32,
                z: z as f32,
            };
            match self.block_at(x, y, z).block_type {
                1 => {
                    instances.push(Instance {
                        position,
                        rotation: null_rotation,
                        atlas_offsets: Cube::grass_atlas_offsets(),
                    });
                }
                2 => {
                    instances.push(Instance {
                        position,
                        rotation: null_rotation,
                        atlas_offsets: Cube::dirt_atlas_offsets(),
                    });
                }
                _ => (),
            }
        }

        let instance_data = instances.iter().map(Instance::to_raw).collect::<Vec<_>>();

        let elapsed_time = func_start.elapsed().as_millis();
        println!("Took {}ms to generate vertex data", elapsed_time);

        (instances, instance_data)
    }

    // Ray intersection algo pseudocode:
    //   start at eye e
    //   all_candidate_cubes = []
    //   repeat for N steps  # N = 20ish
    //     add unit vector in direction t  # t = target
    //     for all possible intersecting cubes  # possible intersection means we added/subtracted 1 to an axis
    //       add cube to all_candidate_cubes
    //   colliding_cubes = []
    //   for cube in all_candidate_cubes:
    //     if cube doesn't exist, skip
    //     if cube exists
    //       check intersection using ray tracing linear algebra  # https://www.scratchapixel.com/lessons/3d-basic-rendering/minimal-ray-tracer-rendering-simple-shapes/ray-box-intersection
    //       if intersection
    //         add to colliding cubes
    //         only iterate 6 more times  # optimization
    //   pick closest colliding cube to camera eye
    //   break cube
    pub fn break_block(&mut self, camera: &super::camera::Camera) {
        use cgmath_17::{InnerSpace, Point3};
        let mut all_candidate_cubes: Vec<Point3<f32>> = vec![];

        let camera_eye_cgmath17 = Point3::new(camera.eye.x, camera.eye.y, camera.eye.z);
        all_candidate_cubes.push(Point3::new(
            camera_eye_cgmath17.x.floor(),
            camera_eye_cgmath17.y.floor(),
            camera_eye_cgmath17.z.floor(),
        ));

        let camera_target_cgmath17 = Point3::new(camera.target.x, camera.target.y, camera.target.z);

        let forward_unit = (camera_target_cgmath17 - camera_eye_cgmath17).normalize();

        let x_dir = forward_unit.x.signum();
        let y_dir = forward_unit.y.signum();
        let z_dir = forward_unit.z.signum();

        let mut curr_pos = camera_eye_cgmath17;

        const MAX_ITER: usize = 20;
        for _ in 0..MAX_ITER {
            curr_pos += forward_unit;
            let cube = Point3::new(curr_pos.x.floor(), curr_pos.y.floor(), curr_pos.z.floor());

            // Add all possible intersecting neighbors as the ray moves forward
            for (x_diff, y_diff, z_diff) in iproduct!([0.0, -x_dir], [0.0, -y_dir], [0.0, -z_dir]) {
                all_candidate_cubes.push(Point3::new(
                    cube.x + x_diff,
                    cube.y + y_diff,
                    cube.z + z_diff,
                ));
            }

            all_candidate_cubes.push(cube);
        }

        let collision_ray = collision::Ray::new(camera_eye_cgmath17, forward_unit);

        let mut closest_collider: (f32 /* closest distance */, [usize; 3]) =
            (std::f32::INFINITY, [0, 0, 0]);
        let mut hit_first_collision = false;
        let mut additional_checks = 0;

        for cube in all_candidate_cubes.iter() {
            let collision_cube = collision::Aabb3::new(
                *cube,
                cgmath_17::Point3::new(cube.x + 1.0, cube.y + 1.0, cube.z + 1.0),
            );

            if self
                .block_at(cube.x as usize, cube.y as usize, cube.z as usize)
                .block_type
                != 0
            {
                let maybe_collision = collision_ray.intersection(&collision_cube);

                if let Some(ref collision_point) = maybe_collision {
                    hit_first_collision = true;
                    let collision_distance = collision_point.distance(camera_eye_cgmath17);
                    if collision_distance < closest_collider.0 {
                        closest_collider = (
                            collision_distance,
                            [cube.x as usize, cube.y as usize, cube.z as usize],
                        )
                    }
                }
            }
            if hit_first_collision {
                additional_checks += 1;
            }
            // TODO: should this be 7???
            if additional_checks > 6 {
                break;
            }
        }

        self.set_block(
            closest_collider.1[0],
            closest_collider.1[1],
            closest_collider.1[2],
            0,
        )
    }
}
