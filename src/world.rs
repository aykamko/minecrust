use crate::camera::Camera;
use crate::map_generation::{self, save_elevation_to_file};
use crate::vec_extra::{Vec2d, Vec3d};
use crate::ChunkRenderDescriptor;
use bitmaps::Bitmap;
use itertools::Itertools;

use super::instance::{Instance, InstanceRaw};
use cgmath::{prelude::*, MetricSpace, Point2, Point3};
use collision::Continuous;
use std::convert::Into;
use std::default;
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

#[derive(PartialEq)]
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

// The largest the world can be in xz dimension
pub const MAX_CHUNK_WORLD_WIDTH: usize = 1024;
// How many chunks are visible in xz dimension
pub const VISIBLE_CHUNK_WIDTH: usize = 16;

const CHUNK_DOES_NOT_EXIST_VALUE: u32 = u32::max_value();
pub const NO_RENDER_DESCRIPTOR_INDEX: usize = usize::max_value();

const MIN_HEIGHT: u16 = 2;
const MAX_HEIGHT: u16 = 80;
const WATER_HEIGHT: u16 = 26;

// Goal: infinite world generation

// Today: whole world is represented as one contiguous 3D array
//   I can't resize this array in 3D -- that doesn't make sense
// Maybe solution?: move to an array of chunks, where each chunk knows it's x/z position in world space
//   Ok -- how will I quickly index on a particular x/z chunk in a flat array?
//     Need a separate 2D array with some absurd size (1024 x 1024 chunks). Each element is an index to the flat array

impl Default for Block {
    fn default() -> Block {
        Block {
            block_type: BlockType::Empty,
            neighbors: NeighborBitmap::new(),
        }
    }
}

pub fn get_world_center() -> Point3<usize> {
    Point3::new(
        MAX_CHUNK_WORLD_WIDTH * CHUNK_XZ_SIZE / 2,
        40,
        MAX_CHUNK_WORLD_WIDTH * CHUNK_XZ_SIZE / 2,
    )
}


#[derive(Clone, PartialEq, Debug)]
pub enum ChunkDataType {
    Opaque,
    Transluscent,
}

#[derive(Clone)]
pub struct TypedInstances {
    pub data_type: ChunkDataType,
    pub instance_data: Vec<InstanceRaw>,
}

#[derive(Clone)]
pub struct ChunkData {
    pub position: [usize; 2],
    // Position of chunk relative to camera (16x16 grid of chunks)
    // TODO: find a better name
    pub camera_relative_position: [usize; 2],
    pub typed_instances_vec: Vec<TypedInstances>,
}

pub struct Chunk {
    position: [usize; 2],
    blocks: Vec3d<Block>,
    // Index into RenderDescriptor array for rendering this chunk
    pub render_descriptor_idx: usize,
}

pub struct WorldState {
    pub chunk_indices: Vec2d<u32>,
    chunks: Vec<Chunk>,
}

impl WorldState {
    pub fn new() -> Self {
        Self {
            chunk_indices: Vec2d::new(
                vec![CHUNK_DOES_NOT_EXIST_VALUE; MAX_CHUNK_WORLD_WIDTH * MAX_CHUNK_WORLD_WIDTH],
                [MAX_CHUNK_WORLD_WIDTH, MAX_CHUNK_WORLD_WIDTH],
            ),
            chunks: vec![],
        }
    }

    fn get_chunk_mut(&mut self, chunk_idx: [usize; 2]) -> &mut Chunk {
        let chunk_idx = self.chunk_indices[chunk_idx];
        &mut self.chunks[chunk_idx as usize]
    }

    fn get_chunk(&self, chunk_idx: [usize; 2]) -> &Chunk {
        let chunk_idx = self.chunk_indices[chunk_idx];
        &self.chunks[chunk_idx as usize]
    }

    fn get_block_mut(&mut self, x: usize, y: usize, z: usize) -> &mut Block {
        let chunk_idx = self.chunk_indices[[x / CHUNK_XZ_SIZE, z / CHUNK_XZ_SIZE]];
        let block_idx = [x % CHUNK_XZ_SIZE, y, z % CHUNK_XZ_SIZE];
        let chunk = &mut self.chunks[chunk_idx as usize];
        &mut chunk.blocks[block_idx]
    }

    fn get_block(&self, x: usize, y: usize, z: usize) -> &Block {
        let chunk_idx = self.chunk_indices[[x / CHUNK_XZ_SIZE, z / CHUNK_XZ_SIZE]];
        // println!("fetched chunk {:?}", [x / CHUNK_XZ_SIZE, z / CHUNK_XZ_SIZE]);
        let block_idx = [x % CHUNK_XZ_SIZE, y, z % CHUNK_XZ_SIZE];
        let chunk = &self.chunks[chunk_idx as usize];
        &chunk.blocks[block_idx]
    }

    fn set_block(&mut self, x: usize, y: usize, z: usize, mut block_type: BlockType) {
        struct Neighbor {
            pos: [usize; 3],
            block: Block,
            this_shared_face: Face,
            other_shared_face: Face,
        }

        let mut neighbors: Vec<Neighbor> = vec![];

        if y < CHUNK_Y_SIZE {
            neighbors.push(Neighbor {
                pos: [x, y + 1, z],
                block: *self.get_block(x, y + 1, z),
                this_shared_face: Face::Top,
                other_shared_face: Face::Bottom,
            });
        }
        if y > 0 {
            neighbors.push(Neighbor {
                pos: [x, y - 1, z],
                block: *self.get_block(x, y - 1, z),
                other_shared_face: Face::Top,
                this_shared_face: Face::Bottom,
            });
        }
        // TODO: more bounds checks?
        neighbors.push(Neighbor {
            pos: [x + 1, y, z],
            block: *self.get_block(x + 1, y, z),
            other_shared_face: Face::Right,
            this_shared_face: Face::Left,
        });
        neighbors.push(Neighbor {
            pos: [x - 1, y, z],
            block: *self.get_block(x - 1, y, z),
            other_shared_face: Face::Left,
            this_shared_face: Face::Right,
        });
        neighbors.push(Neighbor {
            pos: [x, y, z + 1],
            block: *self.get_block(x, y, z + 1),
            other_shared_face: Face::Back,
            this_shared_face: Face::Front,
        });
        neighbors.push(Neighbor {
            pos: [x, y, z - 1],
            block: *self.get_block(x, y, z - 1),
            other_shared_face: Face::Front,
            this_shared_face: Face::Back,
        });

        let curr_block = self.get_block_mut(x, y, z);

        // If we're breaking a block next to water, fill this block with water instead
        if block_type == BlockType::Empty {
            for neighbor in neighbors.iter() {
                if neighbor.block.block_type == BlockType::Water
                    && neighbor.this_shared_face != Face::Bottom
                {
                    block_type = BlockType::Water;
                }
            }
        }
        curr_block.block_type = block_type;

        for neighbor in neighbors.into_iter() {
            let neighbor_block =
                self.get_block_mut(neighbor.pos[0], neighbor.pos[1], neighbor.pos[2]);

            match (block_type, neighbor_block.block_type) {
                (BlockType::Water, BlockType::Water) => {
                    neighbor_block
                        .neighbors
                        .set(neighbor.other_shared_face, true);
                    self.get_block_mut(x, y, z)
                        .neighbors
                        .set(neighbor.this_shared_face, true);
                }
                // (BlockType::Water, _) => {
                //     if neighbor_block.block_type != BlockType::Empty {
                //         neighbor_block.block_type = BlockType::Sand;
                //     }
                // }
                (_, _) => {
                    neighbor_block
                        .neighbors
                        .set(neighbor.other_shared_face, !block_type.is_transluscent());
                }
            }
        }
    }

    fn maybe_allocate_chunk(&mut self, outer_chunk_idx: [usize; 2]) -> bool {
        let mut allocate_inner = |inner_chunk_idx: [usize; 2]| -> bool {
            if self.chunk_indices[inner_chunk_idx] == CHUNK_DOES_NOT_EXIST_VALUE {
                self.chunks.push(Chunk {
                    position: inner_chunk_idx,
                    blocks: Vec3d::new(
                        vec![
                            Block {
                                ..Default::default()
                            };
                            CHUNK_XZ_SIZE * CHUNK_Y_SIZE * CHUNK_XZ_SIZE
                        ],
                        [CHUNK_XZ_SIZE, CHUNK_Y_SIZE, CHUNK_XZ_SIZE],
                    ),
                    render_descriptor_idx: NO_RENDER_DESCRIPTOR_INDEX,
                });
                self.chunk_indices[inner_chunk_idx] = self.chunks.len() as u32 - 1;
                true
            } else {
                false
            }
        };

        let [chunk_x, chunk_z] = outer_chunk_idx;
        // Allocate neighbors to avoid out-of-bounds array accessing when modifying blocks
        let did_allocate = [
            allocate_inner([chunk_x - 1, chunk_z - 1]),
            allocate_inner([chunk_x - 1, chunk_z]),
            allocate_inner([chunk_x - 1, chunk_z + 1]),
            allocate_inner([chunk_x, chunk_z - 1]),
            allocate_inner([chunk_x, chunk_z]),
            allocate_inner([chunk_x, chunk_z + 1]),
            allocate_inner([chunk_x + 1, chunk_z - 1]),
            allocate_inner([chunk_x + 1, chunk_z]),
            allocate_inner([chunk_x + 1, chunk_z + 1]),
        ]
        .into_iter()
        .reduce(|accum, item| accum || item)
        .unwrap();

        // Generate initial blocks
        let elevation_map = map_generation::generate_chunk_elevation_map(
            [chunk_x, chunk_z],
            MIN_HEIGHT,
            MAX_HEIGHT,
        );
        let (base_x, base_z) = (chunk_x * CHUNK_XZ_SIZE, chunk_z * CHUNK_XZ_SIZE);
        // save_elevation_to_file(map_elevation, "map.bmp");

        for (x, z) in iproduct!(0..CHUNK_XZ_SIZE, 0..CHUNK_XZ_SIZE) {
            let ground_elevation = elevation_map[x][z] as usize;
            let (world_x, world_z) = (base_x + x, base_z + z);
            let top_block_type = if ground_elevation < WATER_HEIGHT as usize {
                BlockType::Sand
            } else {
                BlockType::Grass
            };
            self.set_block(world_x, ground_elevation, world_z, top_block_type);

            for y in 0..core::cmp::min(ground_elevation, WATER_HEIGHT as usize) {
                self.set_block(world_x, y, world_z, BlockType::Sand);
            }
            for y in core::cmp::min(ground_elevation, WATER_HEIGHT as usize)..ground_elevation {
                self.set_block(world_x, y, world_z, BlockType::Dirt);
            }
            for y in (MIN_HEIGHT as usize)..(WATER_HEIGHT as usize) {
                if self.get_block(world_x, y, world_z).block_type == BlockType::Empty {
                    self.set_block(world_x, y, world_z, BlockType::Water);
                }
            }
        }

        did_allocate
    }

    pub fn initial_setup(&mut self) {
        // HACK: we're assuming the camera is at the center of the world

        // Generate initial chunks in the center of the world
        let first_chunk_xz_index = (MAX_CHUNK_WORLD_WIDTH / 2) - (VISIBLE_CHUNK_WIDTH / 2);
        let last_chunk_xz_index = first_chunk_xz_index + VISIBLE_CHUNK_WIDTH;
        for (chunk_x, chunk_z) in iproduct!(
            first_chunk_xz_index..last_chunk_xz_index,
            first_chunk_xz_index..last_chunk_xz_index
        ) {
            self.maybe_allocate_chunk([chunk_x, chunk_z]);
        }
    }

    pub fn set_render_descriptor_idx(&mut self, chunk_idx: [usize; 2], render_descriptor_idx: usize) {
        let mut chunk = self.get_chunk_mut(chunk_idx);
        chunk.render_descriptor_idx = render_descriptor_idx;
    }

    pub fn get_render_descriptor_idx(&self, chunk_idx: [usize; 2]) -> usize {
        let chunk = self.get_chunk(chunk_idx);
        chunk.render_descriptor_idx
    }

    pub fn get_chunk_order_by_distance(&self, camera: &Camera) -> Vec<[usize; 2]> {
        // let mut chunk_order: Vec<[usize; 2]> = vec![];
        // for (chunk_x, chunk_z) in iproduct!(0..VISIBLE_CHUNK_WIDTH, 0..VISIBLE_CHUNK_WIDTH) {
        //     chunk_order.push([chunk_x, chunk_z]);
        // }

        let mut chunk_order = self.iter_visible_chunks(camera).collect::<Vec<_>>();

        chunk_order.sort_by(|[chunk_a_x, chunk_a_z], [chunk_b_x, chunk_b_z]| {
            let chunk_a_center_pos = cgmath::Point3::new(
                ((chunk_a_x * CHUNK_XZ_SIZE) + (CHUNK_XZ_SIZE / 2)) as f32,
                camera.eye.y,
                ((chunk_a_z * CHUNK_XZ_SIZE) + (CHUNK_XZ_SIZE / 2)) as f32,
            );
            let chunk_a_distance = camera.eye.distance(chunk_a_center_pos);

            let chunk_b_center_pos = cgmath::Point3::new(
                ((chunk_b_x * CHUNK_XZ_SIZE) + (CHUNK_XZ_SIZE / 2)) as f32,
                camera.eye.y,
                ((chunk_b_z * CHUNK_XZ_SIZE) + (CHUNK_XZ_SIZE / 2)) as f32,
            );
            let chunk_b_distance = camera.eye.distance(chunk_b_center_pos);

            chunk_a_distance.partial_cmp(&chunk_b_distance).unwrap()
        });
        // println!("Chunk order is {:?}", chunk_index_order);

        chunk_order
    }

    fn iter_visible_chunks(&self, camera: &Camera) -> std::vec::IntoIter<[usize; 2]> {
        let (camera_chunk_x, camera_chunk_z) = (
            (camera.eye.x / CHUNK_XZ_SIZE as f32) as usize,
            (camera.eye.z / CHUNK_XZ_SIZE as f32) as usize,
        );
        let first_chunk_x_index = camera_chunk_x - (VISIBLE_CHUNK_WIDTH / 2);
        let first_chunk_z_index = camera_chunk_z - (VISIBLE_CHUNK_WIDTH / 2);

        let mut chunk_idxs: Vec<[usize; 2]> = vec![];
        for (chunk_x, chunk_z) in iproduct!(
            first_chunk_x_index..first_chunk_x_index + VISIBLE_CHUNK_WIDTH,
            first_chunk_z_index..first_chunk_z_index + VISIBLE_CHUNK_WIDTH
        ) {
            chunk_idxs.push([chunk_x, chunk_z]);
        }

        chunk_idxs.into_iter()
    }

    fn camera_relative_position_from_world_position(
        &self,
        chunk_idx: [usize; 2],
        camera: &Camera,
    ) -> [usize; 2] {
        let [world_chunk_x, world_chunk_z] = chunk_idx;
        let (camera_chunk_x, camera_chunk_z) = (
            (camera.eye.x / CHUNK_XZ_SIZE as f32) as usize,
            (camera.eye.z / CHUNK_XZ_SIZE as f32) as usize,
        );
        let first_chunk_x_index = camera_chunk_x - (VISIBLE_CHUNK_WIDTH / 2);
        let first_chunk_z_index = camera_chunk_z - (VISIBLE_CHUNK_WIDTH / 2);

        [
            world_chunk_x - first_chunk_x_index,
            world_chunk_z - first_chunk_z_index,
        ]
    }

    pub fn generate_world_data(&mut self, camera: &Camera) -> (Vec2d<ChunkData>, Vec<[usize; 2]>) {
        let func_start = Instant::now();

        let mut all_chunk_data: Vec2d<ChunkData> = Vec2d::new(
            vec![
                ChunkData {
                    position: [0, 0],
                    camera_relative_position: [0, 0],
                    typed_instances_vec: vec![],
                };
                VISIBLE_CHUNK_WIDTH * VISIBLE_CHUNK_WIDTH
            ],
            [VISIBLE_CHUNK_WIDTH, VISIBLE_CHUNK_WIDTH],
        );

        let mut abs_chunk_iter = self.iter_visible_chunks(camera);
        for (rel_chunk_x, rel_chunk_z) in iproduct!(0..VISIBLE_CHUNK_WIDTH, 0..VISIBLE_CHUNK_WIDTH)
        {
            let [abs_chunk_x, abs_chunk_z] = abs_chunk_iter.next().unwrap();
            all_chunk_data[[rel_chunk_x, rel_chunk_z]] =
                self.generate_chunk_data([abs_chunk_x, abs_chunk_z], camera);
        }

        let elapsed_time = func_start.elapsed().as_millis();
        println!(
            "Took {}ms to generate whole world vertex data",
            elapsed_time
        );

        (all_chunk_data, self.get_chunk_order_by_distance(&camera))
    }

    pub fn generate_chunk_data(&mut self, chunk_idx: [usize; 2], camera: &Camera) -> ChunkData {
        let func_start = Instant::now();

        self.maybe_allocate_chunk(chunk_idx);

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

        let mut opaque_instances: Vec<Instance> = vec![];
        let mut transluscent_instances: Vec<Instance> = vec![];

        let [chunk_x, chunk_z] = chunk_idx;
        for (chunk_rel_x, y, chunk_rel_z) in
            iproduct!(0..CHUNK_XZ_SIZE, 0..CHUNK_Y_SIZE, 0..CHUNK_XZ_SIZE)
        {
            let x = (chunk_x * CHUNK_XZ_SIZE) + chunk_rel_x;
            let z = (chunk_z * CHUNK_XZ_SIZE) + chunk_rel_z;

            let position = cgmath::Vector3::new(x as f32, y as f32, z as f32);
            let block = self.get_block(x, y, z);
            if block.block_type == BlockType::Empty {
                continue;
            }

            let distance_from_camera = (camera.eye - cgmath::Vector3::new(0.5, 0.5, 0.5))
                .distance((x as f32, y as f32, z as f32).into());

            let [top_offset, bottom_offset, side_offset] = block.block_type.texture_atlas_offsets();
            let alpha_adjust = if block.block_type == BlockType::Water {
                0.7
            } else {
                1.0
            };

            let instance_vec = if block.block_type.is_transluscent() {
                &mut transluscent_instances
            } else {
                &mut opaque_instances
            };

            if !block.neighbors.get(Face::Top) {
                let y_offset = if block.block_type == BlockType::Water {
                    0.8
                } else {
                    1.0
                };
                instance_vec.push(Instance {
                    position: position + cgmath::Vector3::new(0.0, y_offset, 1.0),
                    rotation: flip_to_top,
                    texture_atlas_offset: top_offset,
                    color_adjust: [1.0, 1.0, 1.0, alpha_adjust],
                    distance_from_camera,
                });
            }
            if !block.neighbors.get(Face::Bottom) {
                instance_vec.push(Instance {
                    position,
                    rotation: no_rotation,
                    texture_atlas_offset: bottom_offset,
                    color_adjust: [1.0, 1.0, 1.0, alpha_adjust],
                    distance_from_camera,
                });
            }
            if !block.neighbors.get(Face::Left) {
                instance_vec.push(Instance {
                    position: position + cgmath::Vector3::new(1.0, 1.0, 0.0),
                    rotation: flip_to_left,
                    texture_atlas_offset: side_offset,
                    color_adjust: [0.7, 0.7, 0.7, alpha_adjust],
                    distance_from_camera,
                });
            }
            if !block.neighbors.get(Face::Right) {
                instance_vec.push(Instance {
                    position: position + cgmath::Vector3::new(0.0, 1.0, 1.0),
                    rotation: flip_to_right,
                    texture_atlas_offset: side_offset,
                    color_adjust: [0.7, 0.7, 0.7, alpha_adjust],
                    distance_from_camera,
                });
            }
            if !block.neighbors.get(Face::Front) {
                instance_vec.push(Instance {
                    position: position + cgmath::Vector3::new(1.0, 1.0, 1.0),
                    rotation: flip_to_back,
                    texture_atlas_offset: side_offset,
                    color_adjust: [0.8, 0.8, 0.8, alpha_adjust],
                    distance_from_camera,
                });
            }
            if !block.neighbors.get(Face::Back) {
                instance_vec.push(Instance {
                    position: position + cgmath::Vector3::new(0.0, 1.0, 0.0),
                    rotation: flip_to_front,
                    texture_atlas_offset: side_offset,
                    color_adjust: [0.8, 0.8, 0.8, alpha_adjust],
                    distance_from_camera,
                });
            }
        }

        let raw_opaque_instances = opaque_instances
            .iter()
            .map(Instance::to_raw)
            .collect::<Vec<_>>();

        let raw_transluscent_instances = transluscent_instances
            .into_iter()
            .sorted_by(|a, b| {
                a.distance_from_camera
                    .partial_cmp(&b.distance_from_camera)
                    .unwrap()
            })
            .iter()
            .rev() // reverse -- we want far blocks drawn first
            // .map(|i| {
            //     let mut adjusted_i = i.clone();
            //     adjusted_i.color_adjust[0] *= 1.0 / i.distance_from_camera.log(2.0);
            //     adjusted_i.color_adjust[1] *= 1.0 / i.distance_from_camera.log(2.0);
            //     adjusted_i.color_adjust[2] *= 1.0 / i.distance_from_camera.log(2.0);
            //     // adjusted_i.color_adjust[3] *= 1.0 / i.distance_from_camera;
            //     Instance::to_raw(&adjusted_i)
            // })
            .map(Instance::to_raw)
            .collect::<Vec<_>>();

        let elapsed_time = func_start.elapsed().as_millis();
        // println!("Took {}ms to generate chunk vertex data", elapsed_time);

        ChunkData {
            position: chunk_idx,
            camera_relative_position: self
                .camera_relative_position_from_world_position(chunk_idx, camera),
            typed_instances_vec: vec![
                TypedInstances {
                    data_type: ChunkDataType::Opaque,
                    instance_data: raw_opaque_instances,
                },
                TypedInstances {
                    data_type: ChunkDataType::Transluscent,
                    instance_data: raw_transluscent_instances,
                },
            ],
        }
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
            let collision_cube =
                collision::Aabb3::new(*cube, Point3::new(cube.x + 1.0, cube.y + 1.0, cube.z + 1.0));

            if !self
                .get_block(cube.x as usize, cube.y as usize, cube.z as usize)
                .block_type
                .is_transluscent()
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
                    && *chunk_x < MAX_CHUNK_WORLD_WIDTH.try_into().unwrap()
                    && *chunk_z >= 0
                    && *chunk_z < MAX_CHUNK_WORLD_WIDTH.try_into().unwrap()
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
