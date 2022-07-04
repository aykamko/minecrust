use crate::camera::Camera;
use crate::map_generation::{self, save_elevation_to_file};
use crate::vec_extra::{Vec2d, Vec3d};
use bitmaps::Bitmap;
use itertools::Itertools;

use super::instance::{Instance, InstanceRaw};
use cgmath::{prelude::*, MetricSpace};
use collision::Continuous;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

#[derive(Copy, Clone, PartialEq)]
#[repr(u8)]
pub enum BlockType {
    Empty,
    Debug,
    Dirt,
    Grass,
    Sand,
    Stone,
    Water,
    Glass,
}

impl BlockType {
    pub fn is_transluscent(&self) -> bool {
        *self == BlockType::Empty || *self == BlockType::Water
    }
}

#[repr(usize)]
enum Face {
    Top = 0,
    Bottom = 1,
    Left = 2,
    Right = 3,
    Front = 4,
    Back = 5,
}

impl BlockType {
    // top, bottom, sides
    fn texture_atlas_offsets(&self) -> [[f32; 2]; 3] {
        match self {
            BlockType::Grass => [[1.0, 0.0], [2.0, 0.0], [0.0, 0.0]],
            BlockType::Dirt => [[2.0, 0.0], [2.0, 0.0], [2.0, 0.0]],
            BlockType::Debug => [[3.0, 0.0], [3.0, 0.0], [3.0, 0.0]],
            BlockType::Sand => [[0.0, 1.0], [0.0, 1.0], [0.0, 1.0]],
            BlockType::Water => [[1.0, 1.0], [1.0, 1.0], [1.0, 1.0]],
            BlockType::Glass => [[2.0, 1.0], [2.0, 1.0], [2.0, 1.0]],
            _ => [[0.0, 0.0], [0.0, 0.0], [0.0, 0.0]],
        }
    }
}

struct BlockCollision {
    distance: f32,
    block_pos: cgmath::Point3<usize>,
    collision_point: cgmath::Point3<f32>,
}

#[derive(Copy, Clone)]
struct NeighborBitmap {
    bitmap: Bitmap<8>,
}

impl NeighborBitmap {
    fn new() -> Self {
        Self {
            bitmap: Bitmap::new(),
        }
    }

    pub fn get(&self, face: Face) -> bool {
        self.bitmap.get(face as usize)
    }

    pub fn set(&mut self, face: Face, value: bool) -> bool {
        self.bitmap.set(face as usize, value)
    }
}

#[derive(Copy, Clone)]
struct Block {
    block_type: BlockType,
    neighbors: NeighborBitmap, // top (+y), bottom (-y), left (+x), right (-x), front (+z), back (-z)
}

pub const CHUNK_XZ_SIZE: usize = 16;
pub const CHUNK_Y_SIZE: usize = 256;
pub const NUM_BLOCKS_IN_CHUNK: usize = CHUNK_XZ_SIZE * CHUNK_Y_SIZE * CHUNK_XZ_SIZE;

pub const WORLD_WIDTH_IN_CHUNKS: usize = 16;
pub const WORLD_XZ_SIZE: usize = CHUNK_XZ_SIZE * WORLD_WIDTH_IN_CHUNKS;
pub const WORLD_Y_SIZE: usize = CHUNK_Y_SIZE;

impl Default for Block {
    fn default() -> Block {
        Block {
            block_type: BlockType::Empty,
            neighbors: NeighborBitmap::new(),
        }
    }
}

pub struct WorldState {
    blocks: Vec3d<Block>,
}

impl WorldState {
    pub fn new() -> Self {
        Self {
            blocks: Vec3d::new(
                vec![
                    Block {
                        ..Default::default()
                    };
                    WORLD_XZ_SIZE * WORLD_Y_SIZE * WORLD_XZ_SIZE
                ],
                [WORLD_XZ_SIZE, WORLD_Y_SIZE, WORLD_XZ_SIZE],
            ),
        }
    }

    fn set_block(&mut self, x: usize, y: usize, z: usize, block_type: BlockType) {
        let curr_block = &mut self.blocks[[x, y, z]];
        curr_block.block_type = block_type;

        let mut set_neighbor_state =
            |neighbor_pos: [usize; 3], neighbor_shared_face: Face, curr_shared_face: Face| {
                let neighbor_block = &mut self.blocks[neighbor_pos];
                if block_type == BlockType::Water && neighbor_block.block_type == BlockType::Water {
                    neighbor_block.neighbors.set(neighbor_shared_face, true);
                    self.blocks[[x, y, z]].neighbors.set(curr_shared_face, true);
                } else {
                    neighbor_block
                        .neighbors
                        .set(neighbor_shared_face, !block_type.is_transluscent());
                }
            };

        if y != WORLD_Y_SIZE - 1 {
            let top_neighbor = [x, y + 1, z];
            set_neighbor_state(top_neighbor, Face::Bottom, Face::Top);
        }
        if y != 0 {
            let bottom_neighbor = [x, y - 1, z];
            set_neighbor_state(bottom_neighbor, Face::Top, Face::Bottom);
        }
        if x != WORLD_XZ_SIZE - 1 {
            let left_neighbor = [x + 1, y, z];
            set_neighbor_state(left_neighbor, Face::Right, Face::Left);
        }
        if x != 0 {
            let right_neighbor = [x - 1, y, z];
            set_neighbor_state(right_neighbor, Face::Left, Face::Right);
        }
        if z != WORLD_XZ_SIZE - 1 {
            let front_neighbor = [x, y, z + 1];
            set_neighbor_state(front_neighbor, Face::Back, Face::Front);
        }
        if z != 0 {
            let back_neighbor = [x, y, z - 1];
            set_neighbor_state(back_neighbor, Face::Front, Face::Back);
        }
    }

    fn block_at(&self, x: usize, y: usize, z: usize) -> &Block {
        &self.blocks[[x, y, z]]
    }

    pub fn initial_setup(&mut self) {
        const MIN_HEIGHT: u16 = 2;
        const MAX_HEIGHT: u16 = 80;
        const WATER_HEIGHT: u16 = 26;

        let map_elevation = map_generation::generate_elevation_map(MIN_HEIGHT, MAX_HEIGHT);
        save_elevation_to_file(map_elevation, "map.bmp");

        for (x, z) in iproduct!(0..WORLD_XZ_SIZE, 0..WORLD_XZ_SIZE) {
            let ground_elevation = map_elevation[x][z] as usize;
            self.set_block(x, ground_elevation, z, BlockType::Grass);
            for y in 0..ground_elevation {
                self.set_block(x, y, z, BlockType::Dirt);
            }
            for y in (MIN_HEIGHT as usize)..(WATER_HEIGHT as usize) {
                if self.block_at(x, y, z).block_type == BlockType::Empty {
                    self.set_block(x, y, z, BlockType::Water);
                }
            }
        }
    }

    pub fn generate_world_data(
        &self,
        camera: &Camera,
    ) -> (Vec2d<Vec<InstanceRaw>>, Vec<[usize; 2]>) {
        let func_start = Instant::now();

        let mut all_raw_instances: Vec2d<Vec<InstanceRaw>> = Vec2d::new(
            vec![vec![]; WORLD_WIDTH_IN_CHUNKS * WORLD_WIDTH_IN_CHUNKS],
            [WORLD_WIDTH_IN_CHUNKS, WORLD_WIDTH_IN_CHUNKS],
        );
        let mut chunk_index_order: Vec<[usize; 2]> = vec![];

        for (chunk_x, chunk_z) in iproduct!(0..WORLD_WIDTH_IN_CHUNKS, 0..WORLD_WIDTH_IN_CHUNKS) {
            all_raw_instances[[chunk_x, chunk_z]] =
                self.generate_chunk_data([chunk_x, chunk_z], camera);
            chunk_index_order.push([chunk_x, chunk_z]);
        }

        let elapsed_time = func_start.elapsed().as_millis();
        println!(
            "Took {}ms to generate whole world vertex data",
            elapsed_time
        );

        chunk_index_order.sort_by(|[chunk_a_x, chunk_a_z], [chunk_b_x, chunk_b_z]| {
            let chunk_a_center_pos = cgmath::Point3::new(
              ((chunk_a_x * CHUNK_XZ_SIZE) + (CHUNK_XZ_SIZE / 2)) as f32,
              camera.eye.y,
              ((chunk_a_z * CHUNK_XZ_SIZE) + (CHUNK_XZ_SIZE / 2)) as f32);
            let chunk_a_distance = camera.eye.distance(chunk_a_center_pos);

            let chunk_b_center_pos = cgmath::Point3::new(
              ((chunk_b_x * CHUNK_XZ_SIZE) + (CHUNK_XZ_SIZE / 2)) as f32,
              camera.eye.y,
              ((chunk_b_z * CHUNK_XZ_SIZE) + (CHUNK_XZ_SIZE / 2)) as f32);
            let chunk_b_distance = camera.eye.distance(chunk_b_center_pos);

            chunk_a_distance.partial_cmp(&chunk_b_distance).unwrap()
        });
        println!("Chunk order is {:?}", chunk_index_order);

        (all_raw_instances, chunk_index_order)
    }

    pub fn generate_chunk_data(&self, chunk_idx: [usize; 2], camera: &Camera) -> Vec<InstanceRaw> {
        let func_start = Instant::now();

        use cgmath::{Deg, Quaternion, Vector3};

        let no_rotation: Quaternion<f32> = Quaternion::from_axis_angle(Vector3::unit_y(), Deg(0.0));
        let flip_to_top: Quaternion<f32> =
            Quaternion::from_axis_angle(Vector3::unit_x(), Deg(180.0));
        let flip_to_front: Quaternion<f32> =
            Quaternion::from_axis_angle(Vector3::unit_x(), Deg(90.0));
        let flip_to_back: Quaternion<f32> =
            Quaternion::from_axis_angle(Vector3::unit_x(), Deg(-90.0))
                * Quaternion::from_axis_angle(Vector3::unit_y(), Deg(180.0));
        let flip_to_left: Quaternion<f32> =
            Quaternion::from_axis_angle(Vector3::unit_z(), Deg(90.0))
                * Quaternion::from_axis_angle(Vector3::unit_y(), Deg(-90.0));
        let flip_to_right: Quaternion<f32> =
            Quaternion::from_axis_angle(Vector3::unit_z(), Deg(-90.0))
                * Quaternion::from_axis_angle(Vector3::unit_y(), Deg(90.0));

        let mut chunk_instances: Vec<Instance> = vec![];
        let [chunk_x, chunk_z] = chunk_idx;

        for (chunk_rel_x, y, chunk_rel_z) in
            iproduct!(0..CHUNK_XZ_SIZE, 0..CHUNK_Y_SIZE, 0..CHUNK_XZ_SIZE)
        {
            let x = (chunk_x * CHUNK_XZ_SIZE) + chunk_rel_x;
            let z = (chunk_z * CHUNK_XZ_SIZE) + chunk_rel_z;

            let position = cgmath::Vector3::new(x as f32, y as f32, z as f32);
            let block = self.block_at(x, y, z);
            if block.block_type == BlockType::Empty {
                continue;
            }

            let distance_from_camera = (camera.eye - cgmath::Vector3::new(0.5, 0.5, 0.5))
                .distance((x as f32, y as f32, z as f32).into());

            let [top_offset, bottom_offset, side_offset] = block.block_type.texture_atlas_offsets();
            let alpha_adjust = if block.block_type == BlockType::Water {
                0.90
            } else {
                1.0
            };

            if !block.neighbors.get(Face::Top) {
                let y_offset = if block.block_type == BlockType::Water {
                    0.8
                } else {
                    1.0
                };
                chunk_instances.push(Instance {
                    position: position + cgmath::Vector3::new(0.0, y_offset, 1.0),
                    rotation: flip_to_top,
                    texture_atlas_offset: top_offset,
                    color_adjust: [1.0, 1.0, 1.0, alpha_adjust],
                    distance_from_camera,
                });
            }
            if !block.neighbors.get(Face::Bottom) {
                chunk_instances.push(Instance {
                    position,
                    rotation: no_rotation,
                    texture_atlas_offset: bottom_offset,
                    color_adjust: [1.0, 1.0, 1.0, alpha_adjust],
                    distance_from_camera,
                });
            }
            if !block.neighbors.get(Face::Left) {
                chunk_instances.push(Instance {
                    position: position + cgmath::Vector3::new(1.0, 1.0, 0.0),
                    rotation: flip_to_left,
                    texture_atlas_offset: side_offset,
                    color_adjust: [0.7, 0.7, 0.7, alpha_adjust],
                    distance_from_camera,
                });
            }
            if !block.neighbors.get(Face::Right) {
                chunk_instances.push(Instance {
                    position: position + cgmath::Vector3::new(0.0, 1.0, 1.0),
                    rotation: flip_to_right,
                    texture_atlas_offset: side_offset,
                    color_adjust: [0.7, 0.7, 0.7, alpha_adjust],
                    distance_from_camera,
                });
            }
            if !block.neighbors.get(Face::Front) {
                chunk_instances.push(Instance {
                    position: position + cgmath::Vector3::new(1.0, 1.0, 1.0),
                    rotation: flip_to_back,
                    texture_atlas_offset: side_offset,
                    color_adjust: [0.8, 0.8, 0.8, alpha_adjust],
                    distance_from_camera,
                });
            }
            if !block.neighbors.get(Face::Back) {
                chunk_instances.push(Instance {
                    position: position + cgmath::Vector3::new(0.0, 1.0, 0.0),
                    rotation: flip_to_front,
                    texture_atlas_offset: side_offset,
                    color_adjust: [0.8, 0.8, 0.8, alpha_adjust],
                    distance_from_camera,
                });
            }
        }

        let raw_chunk_instances = chunk_instances
            .into_iter()
            .sorted_by(|a, b| {
                a.distance_from_camera
                    .partial_cmp(&b.distance_from_camera)
                    .unwrap()
            })
            .iter()
            .rev() // reverse -- we want far blocks drawn first
            .map(Instance::to_raw)
            .collect::<Vec<_>>();

        let elapsed_time = func_start.elapsed().as_millis();
        println!("Took {}ms to generate chunk vertex data", elapsed_time);

        raw_chunk_instances
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
    //
    // Returns colliding cube and colliding point
    fn get_colliding_block(&self, camera: &Camera) -> Option<BlockCollision> {
        use cgmath_17::{InnerSpace, MetricSpace, Point3};
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

        let mut closest_collider = BlockCollision {
            distance: std::f32::INFINITY,
            block_pos: cgmath::Point3::new(0, 0, 0),
            collision_point: cgmath::Point3::new(0.0, 0.0, 0.0),
        };
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
                != BlockType::Empty
            {
                let maybe_collision = collision_ray.intersection(&collision_cube);

                if let Some(ref collision_point) = maybe_collision {
                    hit_first_collision = true;
                    let collision_distance = collision_point.distance(camera_eye_cgmath17);
                    if collision_distance < closest_collider.distance {
                        closest_collider.distance = collision_distance;
                        closest_collider.block_pos =
                            cgmath::Point3::new(cube.x as usize, cube.y as usize, cube.z as usize);
                        closest_collider.collision_point = cgmath::Point3::new(
                            collision_point.x,
                            collision_point.y,
                            collision_point.z,
                        );
                    }
                }
            }
            if hit_first_collision {
                additional_checks += 1;
            }
            if additional_checks >= 7 {
                break;
            }
        }

        if hit_first_collision {
            Some(closest_collider)
        } else {
            None
        }
    }

    fn get_affected_chunks(&self, block_pos: &cgmath::Point3<usize>) -> Vec<[usize; 2]> {
        let (collider_x, collider_z) = (block_pos.x, block_pos.z);
        let (colliding_chunk_x, colliding_chunk_z) = (
            (collider_x / CHUNK_XZ_SIZE) as i32,
            (collider_z / CHUNK_XZ_SIZE) as i32,
        );
        let mut modified_chunks: Vec<[i32; 2]> = vec![[colliding_chunk_x, colliding_chunk_z]];

        // handle neighbor chunks if this block is on the border
        let chunk_rel_collide_x = collider_x % CHUNK_XZ_SIZE;
        let chunk_rel_collide_z = collider_z % CHUNK_XZ_SIZE;
        if chunk_rel_collide_x == 0 {
            modified_chunks.push([colliding_chunk_x - 1, colliding_chunk_z]);
            if chunk_rel_collide_z == 0 {
                modified_chunks.push([colliding_chunk_x - 1, colliding_chunk_z - 1]);
            }
            if chunk_rel_collide_z == CHUNK_XZ_SIZE - 1 {
                modified_chunks.push([colliding_chunk_x - 1, colliding_chunk_z + 1]);
            }
        }
        if chunk_rel_collide_z == 0 {
            modified_chunks.push([colliding_chunk_x, colliding_chunk_z - 1]);
        }
        if chunk_rel_collide_x == CHUNK_XZ_SIZE - 1 {
            modified_chunks.push([colliding_chunk_x + 1, colliding_chunk_z]);
            if chunk_rel_collide_z == 0 {
                modified_chunks.push([colliding_chunk_x + 1, colliding_chunk_z - 1]);
            }
            if chunk_rel_collide_z == CHUNK_XZ_SIZE - 1 {
                modified_chunks.push([colliding_chunk_x + 1, colliding_chunk_z + 1]);
            }
        }
        if chunk_rel_collide_z == CHUNK_XZ_SIZE - 1 {
            modified_chunks.push([colliding_chunk_x, colliding_chunk_z + 1]);
        }

        let affected_chunks = modified_chunks
            .into_iter()
            .filter(|[chunk_x, chunk_z]| {
                *chunk_x >= 0
                    && *chunk_x < WORLD_WIDTH_IN_CHUNKS.try_into().unwrap()
                    && *chunk_z >= 0
                    && *chunk_z < WORLD_WIDTH_IN_CHUNKS.try_into().unwrap()
            })
            .map(|[chunk_x, chunk_z]| [chunk_x as usize, chunk_z as usize])
            .collect();

        // println!("Affected chunks: {:?}", affected_chunks);
        affected_chunks
    }

    // Returns which chunks were modified
    pub fn break_block(&mut self, camera: &Camera) -> Vec<[usize; 2]> {
        let maybe_collision = self.get_colliding_block(camera);
        if let Some(ref collision) = maybe_collision {
            let (collider_x, collider_y, collider_z) = (
                collision.block_pos.x,
                collision.block_pos.y,
                collision.block_pos.z,
            );

            self.set_block(collider_x, collider_y, collider_z, BlockType::Empty);
            println!("collision point is {:?}", collision.collision_point);
            println!("collision block is {:?}", collision.block_pos);

            self.get_affected_chunks(&collision.block_pos)
        } else {
            vec![]
        }
    }

    // Returns which chunks were modified
    pub fn place_block(&mut self, camera: &Camera, block_type: BlockType) -> Vec<[usize; 2]> {
        let maybe_collision = self.get_colliding_block(camera);
        if let Some(ref collision) = maybe_collision {
            println!("collision point is {:?}", collision.collision_point);
            println!("collision block is {:?}", collision.block_pos);

            let mut new_block_pos = cgmath::Point3::<usize>::new(0, 0, 0);
            if collision.collision_point.x - collision.collision_point.x.floor() == 0.0 {
                new_block_pos = cgmath::Point3::new(
                    if collision.collision_point.x as usize == collision.block_pos.x {
                        collision.block_pos.x - 1
                    } else {
                        collision.block_pos.x + 1
                    },
                    collision.block_pos.y,
                    collision.block_pos.z,
                )
            }
            if collision.collision_point.y - collision.collision_point.y.floor() == 0.0 {
                new_block_pos = cgmath::Point3::new(
                    collision.block_pos.x,
                    if collision.collision_point.y as usize == collision.block_pos.y {
                        collision.block_pos.y - 1
                    } else {
                        collision.block_pos.y + 1
                    },
                    collision.block_pos.z,
                )
            }
            if collision.collision_point.z - collision.collision_point.z.floor() == 0.0 {
                new_block_pos = cgmath::Point3::new(
                    collision.block_pos.x,
                    collision.block_pos.y,
                    if collision.collision_point.z as usize == collision.block_pos.z {
                        collision.block_pos.z - 1
                    } else {
                        collision.block_pos.z + 1
                    },
                )
            }
            println!("new block pos is {:?}", collision.block_pos);

            self.set_block(
                new_block_pos.x,
                new_block_pos.y,
                new_block_pos.z,
                block_type,
            );

            self.get_affected_chunks(&new_block_pos)
        } else {
            vec![]
        }
    }
}
