use crate::face::Face;
use bitmaps::Bitmap;

use super::instance::{Instance, InstanceRaw};
use cgmath::prelude::*;
use cgmath_17::MetricSpace;
use collision::Continuous;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

#[derive(Copy, Clone)]
struct Block {
    // TODO: should be an enum
    block_type: u8,
    neighbors: Bitmap<8>, // top (+y), bottom (-y), left (+x), right (-x), front (+z), back (-z)
}

const WORLD_XZ_SIZE: usize = 128;
const WORLD_Y_SIZE: usize = 256;

pub const CHUNK_SIZE_IN_BLOCKS: usize = WORLD_XZ_SIZE * WORLD_XZ_SIZE * WORLD_Y_SIZE;

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
            self.set_block(x, 0, z, 1); // dirt
            self.set_block(x, 1, z, 1); // grass
        }
    }

    pub fn generate_vertex_data(&self) -> (Vec<Instance>, Vec<InstanceRaw>) {
        let func_start = Instant::now();

        use cgmath::{Deg, Quaternion, Vector3};

        let no_rotation = Quaternion::from_axis_angle(Vector3::unit_y(), Deg(0.0));
        let flip_to_top = Quaternion::from_axis_angle(Vector3::unit_x(), Deg(180.0));
        let flip_to_front = Quaternion::from_axis_angle(Vector3::unit_x(), Deg(90.0));
        let flip_to_back = Quaternion::from_axis_angle(Vector3::unit_x(), Deg(-90.0))
            * Quaternion::from_axis_angle(Vector3::unit_y(), Deg(180.0));
        let flip_to_left = Quaternion::from_axis_angle(Vector3::unit_z(), Deg(90.0))
            * Quaternion::from_axis_angle(Vector3::unit_y(), Deg(-90.0));
        let flip_to_right = Quaternion::from_axis_angle(Vector3::unit_z(), Deg(-90.0))
            * Quaternion::from_axis_angle(Vector3::unit_y(), Deg(90.0));

        let mut instances: Vec<Instance> = vec![];

        for (x, y, z) in iproduct!(0..WORLD_XZ_SIZE, 0..WORLD_Y_SIZE, 0..WORLD_XZ_SIZE) {
            let position = cgmath::Vector3::new(x as f32, y as f32, z as f32);
            let block = self.block_at(x, y, z);
            match block.block_type {
                1 => {
                    // bottom
                    if !block.neighbors.get(1) {
                        instances.push(Instance {
                            position,
                            rotation: no_rotation,
                            atlas_offset: [3.0, 0.0],
                        });
                    }
                    // top
                    if !block.neighbors.get(0) {
                        instances.push(Instance {
                            position: position + cgmath::Vector3::new(0.0, 1.0, 1.0),
                            rotation: flip_to_top,
                            atlas_offset: [3.0, 0.0],
                        });
                    }
                    // left
                    if !block.neighbors.get(2) {
                        instances.push(Instance {
                            position: position + cgmath::Vector3::new(1.0, 1.0, 0.0),
                            rotation: flip_to_left,
                            atlas_offset: [3.0, 0.0],
                        });
                    }
                    // right
                    if !block.neighbors.get(3) {
                        instances.push(Instance {
                            position: position + cgmath::Vector3::new(0.0, 1.0, 1.0),
                            rotation: flip_to_right,
                            atlas_offset: [3.0, 0.0],
                        });
                    }
                    // front
                    if !block.neighbors.get(5) {
                        instances.push(Instance {
                            position: position + cgmath::Vector3::new(0.0, 1.0, 0.0),
                            rotation: flip_to_front,
                            atlas_offset: [3.0, 0.0],
                        });
                    }
                    // back
                    if !block.neighbors.get(4) {
                        instances.push(Instance {
                            position: position + cgmath::Vector3::new(1.0, 1.0, 1.0),
                            rotation: flip_to_back,
                            atlas_offset: [3.0, 0.0],
                        });
                    }
                }
                2 => {
                    // instances.push(Instance {
                    //     position,
                    //     rotation: null_rotation,
                    //     atlas_offset: [1.0, 0.0],
                    // });
                    // // bottom
                    // instances.push(Instance {
                    //     position: position + cgmath::Vector3::new(x as f32, y as f32 - 1.0, z as f32),
                    //     rotation: y_flip,
                    //     atlas_offset: [1.0, 0.0],
                    // });
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
