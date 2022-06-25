use cgmath::prelude::*;
use cgmath_17::MetricSpace;
use collision::{Continuous, Discrete};
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
                x: x as f32,
                y: y as f32,
                z: z as f32,
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

    // Ray intersection algo pseudocode:
    //   start at eye e
    //   all_candidate_cubes = []
    //   repeat for N steps  # N = 20ish
    //     add unit vector in direction t  # t = target
    //     candidate_cubes_this_iter = []
    //     for all possible intersecting cubes  # possible intersection means we added +1 to the axis
    //       if cube exists in world
    //         add cube to candidate_cubes_this_iter
    //     all_candidate_cubes.extend(candidate_cubes_this_iter)
    //     if candidate_cubes_this_iter === 7:  # optimization: we had to have hit something here
    //       break
    //   for cube in all_candidate_cubes:
    //     check intersection using ray tracing linear algebra  # https://www.scratchapixel.com/lessons/3d-basic-rendering/minimal-ray-tracer-rendering-simple-shapes/ray-box-intersection
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

        println!("Camera eye is at {:?}", camera.eye);

        let mut curr_pos = camera_eye_cgmath17;

        const MAX_ITER: usize = 20 + 1;
        for _ in 0..MAX_ITER {
            curr_pos += forward_unit;
            let cube = Point3::new(curr_pos.x.floor(), curr_pos.y.floor(), curr_pos.z.floor());
            println!("Adding cube {:?}", cube);

            // Add all possible neighbors as the ray moves forward
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

            if self.blocks[cube.x as usize][cube.y as usize][cube.z as usize].block_type != 0 {
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
            if additional_checks > 6 {
                break;
            }
        }

        self.blocks[closest_collider.1[0]][closest_collider.1[1]][closest_collider.1[2]]
            .block_type = 0;
    }
}
