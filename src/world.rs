use crate::camera::Camera;
use crate::game_loop::GameLoop;
use crate::map_generation::{self};
use crate::vec_extra::{self, Vec2d, Vec3d};
use crate::vertex::{CuboidCoords, QuadListRenderData, Vertex};
use bitmaps::Bitmap;
use na::RealField;
use rand::prelude::SliceRandom;
use winit::event::{ElementState, VirtualKeyCode, WindowEvent};

use nalgebra as na;
use parry3d::shape::{Cuboid, Cylinder};

use super::instance::InstanceRaw;
use cgmath::{prelude::*, MetricSpace, Point3, Vector3};
use collision::Continuous;
use rand::Rng;
use std::collections::HashSet;
use std::convert::Into;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

const VERBOSE_LOGS: bool = false;
macro_rules! vprintln {
    ($($arg:tt)*) => {{
        if VERBOSE_LOGS {
            println!($($arg)*)
        }
    }};
}

#[derive(Copy, Clone, PartialEq, Debug)]
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
    Tree,
    TreeLeaves1,
    TreeLeaves2,
    TreeLeaves3,
    TreeLeaves4,
    RedFlower,
}

impl BlockType {
    pub fn is_semi_transluscent(&self) -> bool {
        match *self {
            BlockType::TreeLeaves1 => true,
            BlockType::TreeLeaves2 => true,
            BlockType::TreeLeaves3 => true,
            BlockType::TreeLeaves4 => true,
            BlockType::RedFlower => true,
            _ => false,
        }
    }

    pub fn is_transluscent(&self) -> bool {
        match *self {
            BlockType::Empty => true,
            BlockType::Water => true,
            BlockType::Glass => true,
            x if x.is_semi_transluscent() => true,
            _ => false,
        }
    }

    pub fn is_collidable(&self) -> bool {
        match *self {
            BlockType::Empty => false,
            BlockType::Water => false,
            BlockType::RedFlower => false,
            _ => true,
        }
    }

    pub fn is_sprite(&self) -> bool {
        match *self {
            BlockType::RedFlower => true,
            _ => false,
        }
    }

    pub fn random_tree_leaf() -> BlockType {
        *[
            Self::TreeLeaves1,
            Self::TreeLeaves2,
            Self::TreeLeaves3,
            Self::TreeLeaves4,
        ]
        .choose(&mut rand::thread_rng())
        .unwrap()
    }

    // top, bottom, sides
    fn texture_atlas_offsets(&self) -> [[f32; 2]; 3] {
        match self {
            BlockType::Grass => [[1.0, 0.0], [2.0, 0.0], [0.0, 0.0]],
            BlockType::Dirt => [[2.0, 0.0], [2.0, 0.0], [2.0, 0.0]],
            BlockType::Debug => [[3.0, 0.0], [3.0, 0.0], [3.0, 0.0]],
            BlockType::Sand => [[0.0, 1.0], [0.0, 1.0], [0.0, 1.0]],
            BlockType::Water => [[1.0, 1.0], [1.0, 1.0], [1.0, 1.0]],
            BlockType::Glass => [[2.0, 1.0], [2.0, 1.0], [2.0, 1.0]],
            BlockType::Tree => [[1.0, 2.0], [1.0, 2.0], [0.0, 2.0]],
            BlockType::TreeLeaves1 => [[0.0, 3.0], [0.0, 3.0], [0.0, 3.0]],
            BlockType::TreeLeaves2 => [[1.0, 3.0], [1.0, 3.0], [1.0, 3.0]],
            BlockType::TreeLeaves3 => [[0.0, 4.0], [0.0, 4.0], [0.0, 4.0]],
            BlockType::TreeLeaves4 => [[1.0, 3.0], [1.0, 3.0], [1.0, 3.0]],
            BlockType::RedFlower => [[2.0, 2.0], [2.0, 2.0], [2.0, 2.0]],
            _ => [[0.0, 0.0], [0.0, 0.0], [0.0, 0.0]],
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
#[repr(usize)]
enum Face {
    Top = 0,
    Bottom = 1,
    Left = 2,
    Right = 3,
    Front = 4,
    Back = 5,
}

pub struct BlockCollision {
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

impl Block {
    pub fn is_empty(&self) -> bool {
        self.block_type == BlockType::Empty
    }
}

pub const CHUNK_XZ_SIZE: usize = 16;
pub const CHUNK_Y_SIZE: usize = 256;
pub const NUM_BLOCKS_IN_CHUNK: usize = CHUNK_XZ_SIZE * CHUNK_Y_SIZE * CHUNK_XZ_SIZE;

cfg_if::cfg_if! {
    if #[cfg(target_arch = "wasm32")] {
// The largest the world can be in xz dimension
pub const MAX_CHUNK_WORLD_WIDTH: usize = 1024;
// How many chunks are visible in xz dimension
pub const VISIBLE_CHUNK_WIDTH: usize = 16;
    } else {
// The largest the world can be in xz dimension
pub const MAX_CHUNK_WORLD_WIDTH: usize = 1024;
// How many chunks are visible in xz dimension
pub const VISIBLE_CHUNK_WIDTH: usize = 32;
    }
}

const CHUNK_DOES_NOT_EXIST_VALUE: u32 = u32::max_value();
pub const NO_RENDER_DESCRIPTOR_INDEX: usize = usize::max_value();

const MIN_HEIGHT: u16 = 2;
const MAX_HEIGHT: u16 = 80;
const WATER_HEIGHT: u16 = 26;

const MAX_BREAK_DISTANCE: usize = 6;

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

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ChunkDataType {
    Opaque,
    Transluscent,
    // Still generates a shadow
    SemiTransluscent,
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
    is_generated: bool,
    blocks: Vec3d<Block, vec_extra::XYZ<CHUNK_XZ_SIZE, CHUNK_Y_SIZE, CHUNK_XZ_SIZE>>,
    // Index into RenderDescriptor array for rendering this chunk
    pub render_descriptor_idx: usize,
}

pub struct CharacterEntity {
    position: glam::Vec3, // center of the cylinder
    velocity: glam::Vec3,
    acceleration: glam::Vec3,
    contacting_floor: bool,
}

impl CharacterEntity {
    pub fn vertex_data(&self) -> QuadListRenderData {
        let mut result_vertex_data = QuadListRenderData {
            vertex_data: vec![],
            index_data: vec![],
        };
        Vertex::generate_quad_data_for_cuboid(
            &CuboidCoords {
                left: self.position.x - 0.5,
                right: self.position.x + 0.5,
                bottom: self.position.y - 1.0,
                top: self.position.y + 1.0,
                near: self.position.z - 0.5,
                far: self.position.z + 0.5,
            },
            None,
            &mut result_vertex_data,
        );
        return result_vertex_data;
    }
}

#[derive(PartialEq)]
enum ButtonState {
    Pressed,
    Held,
    Released,
    Idle,
}

struct InputState {
    is_forward_pressed: bool,
    is_backward_pressed: bool,
    is_left_pressed: bool,
    is_right_pressed: bool,
    jump_button_state: ButtonState,
}

pub struct WorldState {
    pub chunk_indices: Vec2d<u32>,
    chunks: Vec<Chunk>,
    highlighted_chunk: Option<[usize; 2]>,
    highlighted_block: Option<[usize; 3]>,

    pub character_entity: CharacterEntity,
    input_state: InputState,
}

macro_rules! set_block {
    ($self:ident, $x:expr, $y:expr, $z:expr, $block_type:expr) => {
        $self.set_block($x, $y, $z, $block_type, false)
    };
    ($self:ident, $x:expr, $y:expr, $z:expr, $block_type:expr, $verbose:expr) => {
        $self.set_block($x, $y, $z, $block_type, $verbose)
    };
}

impl WorldState {
    pub fn new() -> Self {
        let world_center = get_world_center();

        // let GRAVITY_ACCELERATION = glam::Vec3::new(0.0, -0.0005, 0.0);

        let character_entity = CharacterEntity {
            position: glam::Vec3::new(
                world_center.x as f32 - 20.0,
                world_center.y as f32 + 10.0,
                world_center.z as f32 - 20.0,
            ),
            velocity: glam::Vec3::new(0.0, 0.0, 0.0),
            acceleration: glam::Vec3::new(0.0, 0.0, 0.0),
            contacting_floor: false,
        };

        Self {
            chunk_indices: Vec2d::new(
                vec![CHUNK_DOES_NOT_EXIST_VALUE; MAX_CHUNK_WORLD_WIDTH * MAX_CHUNK_WORLD_WIDTH],
                [MAX_CHUNK_WORLD_WIDTH, MAX_CHUNK_WORLD_WIDTH],
            ),
            chunks: vec![],
            highlighted_chunk: None,
            highlighted_block: None,
            character_entity,
            input_state: InputState {
                is_forward_pressed: false,
                is_backward_pressed: false,
                is_left_pressed: false,
                is_right_pressed: false,
                jump_button_state: ButtonState::Idle,
            },
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

    fn get_block(&self, x: usize, y: usize, z: usize) -> &Block {
        let chunk_idx = self.chunk_indices[[x / CHUNK_XZ_SIZE, z / CHUNK_XZ_SIZE]];
        let chunk = &self.chunks[chunk_idx as usize];
        chunk
            .blocks
            .get_unchecked(x % CHUNK_XZ_SIZE, y, z % CHUNK_XZ_SIZE)
    }

    fn set_block(
        &mut self,
        world_x: usize,
        y: usize,
        world_z: usize,
        mut block_type: BlockType,
        verbose: bool,
    ) {
        unsafe {
            let [chunk_x, chunk_z] = [world_x / CHUNK_XZ_SIZE, world_z / CHUNK_XZ_SIZE];
            let (x, z) = (world_x % CHUNK_XZ_SIZE, world_z % CHUNK_XZ_SIZE);

            let this_block = self
                .get_chunk_mut([chunk_x, chunk_z])
                .blocks
                .get_raw_ptr_mut(x, y, z);

            #[derive(Clone, Copy)]
            struct Neighbor {
                block: *mut Block,
                this_shared_face: Face,
                other_shared_face: Face,
            }

            let mut neighbors: [Option<Neighbor>; 6] = [None; 6];

            if y < CHUNK_Y_SIZE - 1 {
                neighbors[0] = Some(Neighbor {
                    block: self
                        .get_chunk_mut([chunk_x, chunk_z])
                        .blocks
                        .get_raw_ptr_mut(x, y + 1, z),
                    this_shared_face: Face::Top,
                    other_shared_face: Face::Bottom,
                });
            }
            if y > 0 {
                neighbors[1] = Some(Neighbor {
                    block: self
                        .get_chunk_mut([chunk_x, chunk_z])
                        .blocks
                        .get_raw_ptr_mut(x, y - 1, z),
                    other_shared_face: Face::Top,
                    this_shared_face: Face::Bottom,
                });
            }

            neighbors[2] = Some(Neighbor {
                block: if x < CHUNK_XZ_SIZE - 1 {
                    self.get_chunk_mut([chunk_x, chunk_z])
                        .blocks
                        .get_raw_ptr_mut(x + 1, y, z)
                } else {
                    self.get_chunk_mut([chunk_x + 1, chunk_z])
                        .blocks
                        .get_raw_ptr_mut(0, y, z)
                },
                other_shared_face: Face::Right,
                this_shared_face: Face::Left,
            });
            neighbors[3] = Some(Neighbor {
                block: if x > 0 {
                    self.get_chunk_mut([chunk_x, chunk_z])
                        .blocks
                        .get_raw_ptr_mut(x - 1, y, z)
                } else {
                    self.get_chunk_mut([chunk_x - 1, chunk_z])
                        .blocks
                        .get_raw_ptr_mut(CHUNK_XZ_SIZE - 1, y, z)
                },
                other_shared_face: Face::Left,
                this_shared_face: Face::Right,
            });
            neighbors[4] = Some(Neighbor {
                block: if z < CHUNK_XZ_SIZE - 1 {
                    self.get_chunk_mut([chunk_x, chunk_z])
                        .blocks
                        .get_raw_ptr_mut(x, y, z + 1)
                } else {
                    self.get_chunk_mut([chunk_x, chunk_z + 1])
                        .blocks
                        .get_raw_ptr_mut(x, y, 0)
                },
                other_shared_face: Face::Back,
                this_shared_face: Face::Front,
            });
            neighbors[5] = Some(Neighbor {
                block: if z > 0 {
                    self.get_chunk_mut([chunk_x, chunk_z])
                        .blocks
                        .get_raw_ptr_mut(x, y, z - 1)
                } else {
                    self.get_chunk_mut([chunk_x, chunk_z - 1])
                        .blocks
                        .get_raw_ptr_mut(x, y, CHUNK_XZ_SIZE - 1)
                },
                other_shared_face: Face::Front,
                this_shared_face: Face::Back,
            });

            // If we're breaking a block next to water, fill this block with water instead
            if block_type == BlockType::Empty {
                for i in 0..6 {
                    if let Some(neighbor) = neighbors[i] {
                        if (*neighbor.block).block_type == BlockType::Water
                            && neighbor.this_shared_face != Face::Bottom
                        {
                            block_type = BlockType::Water;
                        }
                    }
                }
            }
            if verbose {
                println!(
                    "Setting block @ {:?} from {:?} to {:?}",
                    [x, y, z],
                    self.get_block(x, y, z).block_type,
                    block_type
                );
            }

            (*this_block).block_type = block_type;
            for i in 0..6 {
                let neighbor = match neighbors[i] {
                    Some(neighbor) => neighbor,
                    None => {
                        continue;
                    }
                };
                match (block_type, (*neighbor.block).block_type) {
                    (BlockType::Water, BlockType::Water) => {
                        (*this_block).neighbors.set(neighbor.this_shared_face, true);
                        (*neighbor.block)
                            .neighbors
                            .set(neighbor.other_shared_face, true);
                    }
                    (x, BlockType::Water) if !x.is_transluscent() => {
                        (*this_block)
                            .neighbors
                            .set(neighbor.this_shared_face, false);
                        (*neighbor.block)
                            .neighbors
                            .set(neighbor.other_shared_face, true);
                    }
                    (_, _) => {
                        (*neighbor.block)
                            .neighbors
                            .set(neighbor.other_shared_face, !block_type.is_transluscent());
                    }
                }
            }
        }
    }

    pub fn find_chunk_neighbors(
        &self,
        chunks: &Vec<[usize; 2]>,
        neighbor_candidates: &Vec<[usize; 2]>,
    ) -> Vec<[usize; 2]> {
        let mut possible_neighbors: HashSet<[usize; 2]> = HashSet::new();
        for [chunk_x, chunk_z] in chunks.iter() {
            possible_neighbors.insert([*chunk_x + 1, *chunk_z]);
            possible_neighbors.insert([*chunk_x - 1, *chunk_z]);
            possible_neighbors.insert([*chunk_x, *chunk_z + 1]);
            possible_neighbors.insert([*chunk_x, *chunk_z - 1]);
        }

        neighbor_candidates
            .iter()
            .cloned()
            .filter(|chunk_idx| possible_neighbors.contains(chunk_idx))
            .collect::<Vec<_>>()
    }

    pub fn maybe_generate_tree(&mut self, base_location: [usize; 3]) -> bool {
        const TREE_CHANCE: f32 = 1.0 / 200.0;
        if rand::thread_rng().gen::<f32>() > TREE_CHANCE {
            return false;
        }

        let mut trunk_base = base_location;
        trunk_base[1] += 1;

        let [x, y_base, z] = trunk_base;
        let trunk_top = y_base + 6;
        for y in y_base..trunk_top {
            set_block!(self, x, y, z, BlockType::Tree);
        }

        // Minecraft Lollipop Spruce Tree
        let leaf_slice_diameters = [7, 7, 5, 3];
        let mut leaf_y = trunk_top - 3;
        for diam in leaf_slice_diameters {
            let radius = (diam - 1) / 2;
            for (leaf_x, leaf_z) in
                iproduct!(x - radius + 1..x + radius, z - radius + 1..z + radius)
            {
                // TODO(aleks): need a "set block if empty" primitive
                if self.get_block(leaf_x, leaf_y, leaf_z).is_empty() {
                    set_block!(self, leaf_x, leaf_y, leaf_z, BlockType::random_tree_leaf());
                }
            }
            leaf_y += 1;
        }
        true
    }

    pub fn maybe_generate_flower(&mut self, ground_elevation: [usize; 3]) -> bool {
        const FLOWER_CHANCE: f32 = 1.0 / 100.0;
        if rand::thread_rng().gen::<f32>() > FLOWER_CHANCE {
            return false;
        }

        let [x, y_base, z] = ground_elevation;
        set_block!(self, x, y_base + 1, z, BlockType::RedFlower);
        true
    }

    pub fn generate_chunk(&mut self, [chunk_x, chunk_z]: [usize; 2]) {
        let elevation_map = map_generation::generate_chunk_elevation_map(
            [chunk_x, chunk_z],
            MIN_HEIGHT,
            MAX_HEIGHT,
        );
        let (base_x, base_z) = (chunk_x * CHUNK_XZ_SIZE, chunk_z * CHUNK_XZ_SIZE);
        // vprintln!(
        //     "Took {}ms to generate elevation map",
        //     func_start.elapsed().as_millis()
        // );

        for (z, x) in iproduct!(0..CHUNK_XZ_SIZE, 0..CHUNK_XZ_SIZE) {
            let ground_elevation = elevation_map[x][z] as usize;
            let (world_x, world_z) = (base_x + x, base_z + z);
            let top_block_type = if ground_elevation < WATER_HEIGHT as usize {
                BlockType::Sand
            } else {
                BlockType::Grass
            };
            set_block!(self, world_x, ground_elevation, world_z, top_block_type);

            let min_ground_or_water = core::cmp::min(ground_elevation, WATER_HEIGHT as usize);
            for y in 0..min_ground_or_water {
                set_block!(self, world_x, y, world_z, BlockType::Sand);
            }
            for y in min_ground_or_water..ground_elevation {
                set_block!(self, world_x, y, world_z, BlockType::Dirt);
            }
            for y in (MIN_HEIGHT as usize)..(WATER_HEIGHT as usize) {
                if self.get_block(world_x, y, world_z).block_type == BlockType::Empty {
                    set_block!(self, world_x, y, world_z, BlockType::Water);
                }
            }

            if top_block_type == BlockType::Grass {
                let did_generate_tree =
                    self.maybe_generate_tree([world_x, ground_elevation, world_z]);
                if !did_generate_tree {
                    self.maybe_generate_flower([world_x, ground_elevation, world_z]);
                }
            }
        }

        self.get_chunk_mut([chunk_x, chunk_z]).is_generated = true;
    }

    pub fn maybe_allocate_chunk(&mut self, outer_chunk_idx: [usize; 2]) {
        #[cfg(not(target_arch = "wasm32"))]
        let func_start = Instant::now();

        let mut allocate_inner = |inner_chunk_idx: [usize; 2]| {
            if self.chunk_indices[inner_chunk_idx] == CHUNK_DOES_NOT_EXIST_VALUE {
                let new_chunk = Chunk {
                    is_generated: false,
                    blocks: Vec3d::new(vec![
                        Block {
                            ..Default::default()
                        };
                        CHUNK_XZ_SIZE * CHUNK_Y_SIZE * CHUNK_XZ_SIZE
                    ]),
                    render_descriptor_idx: NO_RENDER_DESCRIPTOR_INDEX,
                };
                self.chunks.push(new_chunk);
                self.chunk_indices[inner_chunk_idx] = self.chunks.len() as u32 - 1;
            }
        };

        let [chunk_x, chunk_z] = outer_chunk_idx;
        // Allocate neighbors to avoid out-of-bounds array accessing when modifying blocks
        allocate_inner([chunk_x - 1, chunk_z - 1]);
        allocate_inner([chunk_x - 1, chunk_z]);
        allocate_inner([chunk_x - 1, chunk_z + 1]);
        allocate_inner([chunk_x, chunk_z - 1]);
        allocate_inner([chunk_x, chunk_z]);
        allocate_inner([chunk_x, chunk_z + 1]);
        allocate_inner([chunk_x + 1, chunk_z - 1]);
        allocate_inner([chunk_x + 1, chunk_z]);
        allocate_inner([chunk_x + 1, chunk_z + 1]);

        #[cfg(not(target_arch = "wasm32"))]
        vprintln!(
            "Took {}ms to allocate memory",
            func_start.elapsed().as_millis()
        );

        if !self.get_chunk(outer_chunk_idx).is_generated {
            self.generate_chunk(outer_chunk_idx)
        }

        #[cfg(not(target_arch = "wasm32"))]
        vprintln!(
            "Took {}ms to process elevation map",
            func_start.elapsed().as_millis()
        );
    }

    pub fn initial_setup(&mut self, camera: &Camera) {
        // Generate initial chunks around initial camera position
        for chunk_idx in self.iter_visible_chunks(camera) {
            self.maybe_allocate_chunk(chunk_idx);
        }
    }

    pub fn set_render_descriptor_idx(
        &mut self,
        chunk_idx: [usize; 2],
        render_descriptor_idx: usize,
    ) {
        let mut chunk = self.get_chunk_mut(chunk_idx);
        chunk.render_descriptor_idx = render_descriptor_idx;
    }

    pub fn get_render_descriptor_idx(&self, chunk_idx: [usize; 2]) -> usize {
        let chunk = self.get_chunk(chunk_idx);
        chunk.render_descriptor_idx
    }

    pub fn get_chunk_order_by_distance(&self, camera: &Camera) -> Vec<[usize; 2]> {
        let mut chunk_order = self.iter_visible_chunks(camera).collect::<Vec<_>>();

        let camera_chunk_pos = cgmath::Point2::<f32>::new(
            camera.eye.x / (CHUNK_XZ_SIZE as f32),
            camera.eye.z / (CHUNK_XZ_SIZE as f32),
        );
        chunk_order.sort_by(|[chunk_a_x, chunk_a_z], [chunk_b_x, chunk_b_z]| {
            let chunk_a_pos = cgmath::Point2::<f32>::new(*chunk_a_x as f32, *chunk_a_z as f32);
            let chunk_a_distance = camera_chunk_pos.distance(chunk_a_pos);
            let chunk_b_pos = cgmath::Point2::<f32>::new(*chunk_b_x as f32, *chunk_b_z as f32);
            let chunk_b_distance = camera_chunk_pos.distance(chunk_b_pos);

            chunk_a_distance.partial_cmp(&chunk_b_distance).unwrap()
        });
        // println!(
        //     "Camera chunk pos is {:?}",
        //     [(camera.eye.x as usize) / CHUNK_XZ_SIZE, (camera.eye.z as usize) / CHUNK_XZ_SIZE]
        // );
        // println!("Chunk order is {:?}", chunk_order);

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
        #[cfg(not(target_arch = "wasm32"))]
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
                self.compute_chunk_mesh([abs_chunk_x, abs_chunk_z], camera);
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let elapsed_time = func_start.elapsed().as_millis();
            println!(
                "Took {}ms to generate whole world vertex data",
                elapsed_time
            );
        }

        (all_chunk_data, self.get_chunk_order_by_distance(&camera))
    }

    pub fn compute_chunk_mesh(&mut self, chunk_idx: [usize; 2], camera: &Camera) -> ChunkData {
        self.maybe_allocate_chunk(chunk_idx);

        use cgmath::{Deg, Quaternion};

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

        let flip_to_diagonal_right_front: Quaternion<f32> =
            Quaternion::from_axis_angle(Vector3::unit_x(), Deg(90.0))
                * Quaternion::from_axis_angle(Vector3::unit_z(), Deg(45.0));
        let flip_to_diagonal_left_front: Quaternion<f32> =
            Quaternion::from_axis_angle(Vector3::unit_x(), Deg(-90.0))
                * Quaternion::from_axis_angle(Vector3::unit_y(), Deg(180.0))
                * Quaternion::from_axis_angle(Vector3::unit_z(), Deg(-45.0));
        let flip_to_diagonal_right_back: Quaternion<f32> =
            Quaternion::from_axis_angle(Vector3::unit_x(), Deg(-90.0))
                * Quaternion::from_axis_angle(Vector3::unit_y(), Deg(180.0))
                * Quaternion::from_axis_angle(Vector3::unit_z(), Deg(45.0));
        let flip_to_diagonal_left_back: Quaternion<f32> =
            Quaternion::from_axis_angle(Vector3::unit_x(), Deg(90.0))
                * Quaternion::from_axis_angle(Vector3::unit_z(), Deg(-45.0));

        let mut opaque_instances = Vec::<InstanceRaw>::with_capacity(4096);
        let mut opaque_instance_distances = Vec::<i32>::with_capacity(4096);

        let mut transluscent_instances = Vec::<InstanceRaw>::with_capacity(4096);
        let mut transluscent_instance_distances = Vec::<i32>::with_capacity(4096);

        let mut semi_transluscent_instances = Vec::<InstanceRaw>::with_capacity(4096);
        let mut semi_transluscent_instance_distances = Vec::<i32>::with_capacity(4096);

        let chunk = self.get_chunk(chunk_idx);

        let [chunk_x, chunk_z] = chunk_idx;

        // Don't use !iproduct here to squeeze out a tiny bit of perf
        for chunk_rel_z in 0..CHUNK_XZ_SIZE {
            for chunk_rel_x in 0..CHUNK_XZ_SIZE {
                for y in 0..CHUNK_Y_SIZE {
                    let world_x = (chunk_x * CHUNK_XZ_SIZE) + chunk_rel_x;
                    let world_z = (chunk_z * CHUNK_XZ_SIZE) + chunk_rel_z;

                    let position = cgmath::Vector3::new(world_x as f32, y as f32, world_z as f32);
                    let block = chunk.blocks.get_unchecked(chunk_rel_x, y, chunk_rel_z);
                    if block.block_type == BlockType::Empty {
                        continue;
                    }

                    let mut highlight_adjust = 1.0;
                    if let Some(highlighted_block) = self.highlighted_block {
                        if highlighted_block == [world_x, y, world_z] {
                            highlight_adjust = 1.3;
                        }
                    }

                    let [top_offset, bottom_offset, side_offset] =
                        block.block_type.texture_atlas_offsets();
                    let alpha_adjust = if block.block_type == BlockType::Water {
                        0.7
                    } else {
                        1.0
                    };

                    let (instance_vec, distance_vec) = if block.block_type.is_semi_transluscent() {
                        (
                            &mut semi_transluscent_instances,
                            &mut semi_transluscent_instance_distances,
                        )
                    } else if block.block_type.is_transluscent() {
                        (
                            &mut transluscent_instances,
                            &mut transluscent_instance_distances,
                        )
                    } else {
                        (&mut opaque_instances, &mut opaque_instance_distances)
                    };

                    let distance_from_camera = (camera.eye - cgmath::Vector3::new(0.5, 0.5, 0.5))
                        .distance((world_x as f32, y as f32, world_z as f32).into());

                    let half_diag_shift = (1.0 - (1.0 / 2.0_f32.sqrt())) / 2.0;

                    if block.block_type.is_sprite() {
                        // left cross, front-face
                        instance_vec.push(InstanceRaw::new(
                            position
                                + cgmath::Vector3::new(1.0 - half_diag_shift, 1.0, half_diag_shift),
                            flip_to_diagonal_left_front,
                            side_offset,
                            (cgmath::Vector4::new(0.7, 0.7, 0.7, alpha_adjust) * highlight_adjust)
                                .into(),
                        ));
                        distance_vec.push(-distance_from_camera as i32);
                        // right cross, front-face
                        instance_vec.push(InstanceRaw::new(
                            position + cgmath::Vector3::new(half_diag_shift, 1.0, half_diag_shift),
                            flip_to_diagonal_right_front,
                            side_offset,
                            (cgmath::Vector4::new(0.7, 0.7, 0.7, alpha_adjust) * highlight_adjust)
                                .into(),
                        ));
                        distance_vec.push(-distance_from_camera as i32);
                        // left cross, back-face
                        instance_vec.push(InstanceRaw::new(
                            position
                                + cgmath::Vector3::new(half_diag_shift, 1.0, 1.0 - half_diag_shift),
                            flip_to_diagonal_left_back,
                            side_offset,
                            (cgmath::Vector4::new(0.7, 0.7, 0.7, alpha_adjust) * highlight_adjust)
                                .into(),
                        ));
                        distance_vec.push(-distance_from_camera as i32);
                        // right cross, back-face
                        instance_vec.push(InstanceRaw::new(
                            position
                                + cgmath::Vector3::new(
                                    1.0 - half_diag_shift,
                                    1.0,
                                    1.0 - half_diag_shift,
                                ),
                            flip_to_diagonal_right_back,
                            side_offset,
                            (cgmath::Vector4::new(0.7, 0.7, 0.7, alpha_adjust) * highlight_adjust)
                                .into(),
                        ));
                        distance_vec.push(-distance_from_camera as i32);
                    } else {
                        if !block.neighbors.get(Face::Top) {
                            let y_offset = if block.block_type == BlockType::Water {
                                0.8
                            } else {
                                1.0
                            };
                            instance_vec.push(InstanceRaw::new(
                                position + cgmath::Vector3::new(0.0, y_offset, 1.0),
                                flip_to_top,
                                top_offset,
                                (cgmath::Vector4::new(1.0, 1.0, 1.0, alpha_adjust)
                                    * highlight_adjust)
                                    .into(),
                            ));

                            // N.B.
                            // - store negative value because we want further instances to be drawn first
                            // - lose float precision to gain speed in sorting (I did not benchmark this, could be useless)
                            distance_vec.push(-distance_from_camera as i32);
                        }
                        if !block.neighbors.get(Face::Bottom) {
                            instance_vec.push(InstanceRaw::new(
                                position,
                                no_rotation,
                                bottom_offset,
                                (cgmath::Vector4::new(1.0, 1.0, 1.0, alpha_adjust)
                                    * highlight_adjust)
                                    .into(),
                            ));
                            distance_vec.push(-distance_from_camera as i32);
                        }
                        if !block.neighbors.get(Face::Left) {
                            instance_vec.push(InstanceRaw::new(
                                position + cgmath::Vector3::new(1.0, 1.0, 0.0),
                                flip_to_left,
                                side_offset,
                                (cgmath::Vector4::new(0.7, 0.7, 0.7, alpha_adjust)
                                    * highlight_adjust)
                                    .into(),
                            ));
                            distance_vec.push(-distance_from_camera as i32);
                        }
                        if !block.neighbors.get(Face::Right) {
                            instance_vec.push(InstanceRaw::new(
                                position + cgmath::Vector3::new(0.0, 1.0, 1.0),
                                flip_to_right,
                                side_offset,
                                (cgmath::Vector4::new(0.7, 0.7, 0.7, alpha_adjust)
                                    * highlight_adjust)
                                    .into(),
                            ));
                            distance_vec.push(-distance_from_camera as i32);
                        }
                        if !block.neighbors.get(Face::Front) {
                            instance_vec.push(InstanceRaw::new(
                                position + cgmath::Vector3::new(1.0, 1.0, 1.0),
                                flip_to_back,
                                side_offset,
                                (cgmath::Vector4::new(0.8, 0.8, 0.8, alpha_adjust)
                                    * highlight_adjust)
                                    .into(),
                            ));
                            distance_vec.push(-distance_from_camera as i32);
                        }
                        if !block.neighbors.get(Face::Back) {
                            instance_vec.push(InstanceRaw::new(
                                position + cgmath::Vector3::new(0.0, 1.0, 0.0),
                                flip_to_front,
                                side_offset,
                                (cgmath::Vector4::new(0.8, 0.8, 0.8, alpha_adjust)
                                    * highlight_adjust)
                                    .into(),
                            ));
                            distance_vec.push(-distance_from_camera as i32);
                        }
                    }
                }
            }
        }

        permutation::sort(&transluscent_instance_distances)
            .apply_slice_in_place(&mut transluscent_instances);
        permutation::sort(&semi_transluscent_instance_distances)
            .apply_slice_in_place(&mut semi_transluscent_instances);
        permutation::sort(&opaque_instance_distances).apply_slice_in_place(&mut opaque_instances);

        ChunkData {
            position: chunk_idx,
            camera_relative_position: self
                .camera_relative_position_from_world_position(chunk_idx, camera),
            typed_instances_vec: vec![
                TypedInstances {
                    data_type: ChunkDataType::Opaque,
                    instance_data: opaque_instances,
                },
                TypedInstances {
                    data_type: ChunkDataType::Transluscent,
                    instance_data: transluscent_instances,
                },
                TypedInstances {
                    data_type: ChunkDataType::SemiTransluscent,
                    instance_data: semi_transluscent_instances,
                },
            ],
        }
    }

    pub fn highlight_colliding_block(&mut self, camera: &Camera) -> Vec<[usize; 2]> {
        let mut modified_chunks: Vec<[usize; 2]> = vec![];

        let prev_highlighted_chunk = self.highlighted_chunk;
        if let Some(chunk_idx) = prev_highlighted_chunk {
            modified_chunks.push(chunk_idx);
        }

        let collision = match self.get_colliding_block(camera, MAX_BREAK_DISTANCE) {
            Some(collision) => collision,
            None => {
                self.highlighted_chunk = None;
                self.highlighted_block = None;

                return modified_chunks;
            }
        };

        let colliding_chunk = [
            (collision.block_pos.x / CHUNK_XZ_SIZE) as usize,
            (collision.block_pos.z / CHUNK_XZ_SIZE) as usize,
        ];
        modified_chunks.push(colliding_chunk);
        self.highlighted_chunk = Some(colliding_chunk);
        self.highlighted_block = Some([
            collision.block_pos.x,
            collision.block_pos.y,
            collision.block_pos.z,
        ]);

        modified_chunks
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
    fn get_colliding_block(&self, camera: &Camera, max_distance: usize) -> Option<BlockCollision> {
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

        for _ in 0..max_distance {
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

            if self
                .get_block(cube.x as usize, cube.y as usize, cube.z as usize)
                .block_type
                .is_collidable()
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

    pub fn block_collidable_at_point(&self, point: &cgmath::Point3<f32>) -> bool {
        let block_at_pos = self.get_block(point.x as usize, point.y as usize, point.z as usize);
        return block_at_pos.block_type.is_collidable();
    }

    pub fn collision_normal_from_ray_2(
        &self,
        camera: &Camera,
        next_eye: &cgmath::Point3<f32>,
    ) -> Option<Vector3<f32>> {
        let distance = (next_eye - camera.eye).magnitude().ceil() as usize;

        return match self.get_colliding_block(camera, distance) {
            Some(collision) => {
                let collision_point = collision.collision_point;
                let block_pos = collision.block_pos;
                // Get the collision normal
                let collision_normal = if collision_point.x - collision_point.x.floor() == 0.0 {
                    if collision_point.x as usize == block_pos.x {
                        Some(Vector3::new(-1.0, 0.0, 0.0))
                    } else {
                        Some(Vector3::new(1.0, 0.0, 0.0))
                    }
                } else if collision_point.y - collision_point.y.floor() == 0.0 {
                    if collision_point.y as usize == block_pos.y {
                        Some(Vector3::new(0.0, -1.0, 0.0))
                    } else {
                        Some(Vector3::new(0.0, 1.0, 0.0))
                    }
                } else if collision_point.z - collision_point.z.floor() == 0.0 {
                    if collision_point.z as usize == block_pos.z {
                        Some(Vector3::new(0.0, 0.0, -1.0))
                    } else {
                        Some(Vector3::new(0.0, 0.0, 1.0))
                    }
                } else {
                    return None;
                };
                // println!("Normal is {:?}", collision_normal);
                return collision_normal;
            }
            None => None,
        };
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
        let maybe_collision = self.get_colliding_block(camera, MAX_BREAK_DISTANCE);
        if let Some(ref collision) = maybe_collision {
            let (collider_x, collider_y, collider_z) = (
                collision.block_pos.x,
                collision.block_pos.y,
                collision.block_pos.z,
            );

            vprintln!(
                "break_block collision point is {:?}",
                collision.collision_point
            );
            vprintln!("break_block collision block is {:?}", collision.block_pos);
            set_block!(self, collider_x, collider_y, collider_z, BlockType::Empty);

            self.get_affected_chunks(&collision.block_pos)
        } else {
            vec![]
        }
    }

    // Returns which chunks were modified
    pub fn place_block(&mut self, camera: &Camera, block_type: BlockType) -> Vec<[usize; 2]> {
        let maybe_collision = self.get_colliding_block(camera, MAX_BREAK_DISTANCE + 1);
        if let Some(ref collision) = maybe_collision {
            vprintln!(
                "place_block collision point is {:?}",
                collision.collision_point
            );
            vprintln!("place_block collision block is {:?}", collision.block_pos);

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
            vprintln!("place_block new block pos is {:?}", collision.block_pos);

            set_block!(
                self,
                new_block_pos.x,
                new_block_pos.y,
                new_block_pos.z,
                block_type
            );

            self.get_affected_chunks(&new_block_pos)
        } else {
            vec![]
        }
    }

    pub fn physics_tick(&mut self, game_loop: &mut GameLoop) {
        let GRAVITY_Y_ACCEL: f32 = (game_loop.fixed_time_step().powi(2) * -9.807) as f32;

        // top, left, far corner of character cylinder
        let floored_position = (
            self.character_entity.position.x.floor() as usize - 1,
            self.character_entity.position.y.floor() as usize + 1,
            self.character_entity.position.z.floor() as usize - 1,
        );
        let mut floor_blocks_to_check_collision: Vec<[usize; 3]> = vec![];
        for (dx, dz) in iproduct!(0..2, 0..2) {
            if self
                .get_block(
                    floored_position.0 + dx,
                    floored_position.1 - 2,
                    floored_position.2 + dz,
                )
                .block_type
                .is_collidable()
            {
                floor_blocks_to_check_collision.push([
                    floored_position.0 + dx,
                    floored_position.1 - 2,
                    floored_position.2 + dz,
                ])
            }
        }
        let character_collider = Cylinder::new(1.0, 0.5);
        let character_pos = na::Isometry3::new(
            na::vector![
                self.character_entity.position.x,
                self.character_entity.position.y,
                self.character_entity.position.z
            ],
            na::zero(),
        );

        let mut floor_contact: Option<parry3d::query::Contact> = None;
        for block_pos in floor_blocks_to_check_collision {
            let block_collider = Cuboid::new(na::vector![0.5, 0.5, 0.5]);
            let block_pos = na::Isometry3::new(
                na::vector![
                    block_pos[0] as f32 + 0.5,
                    block_pos[1] as f32 + 0.5,
                    block_pos[2] as f32 + 0.5
                ],
                na::zero(),
            );

            let maybe_contact = parry3d::query::contact(
                &character_pos,
                &character_collider,
                &block_pos,
                &block_collider,
                0.01, // tolerance
            )
            .unwrap();

            if maybe_contact.is_some() {
                floor_contact = maybe_contact;
                break;
            }
        }

        match floor_contact {
            Some(contact) => {
                self.character_entity.acceleration.y = 0.0;
                self.character_entity.velocity.y = 0.0;

                if !self.character_entity.contacting_floor {
                    let adjust_vec =
                        glam::Vec3::new(contact.normal1.x, contact.normal1.y, contact.normal1.z)
                            * contact.dist;
                    self.character_entity.position += adjust_vec;
                }
                self.character_entity.contacting_floor = true;
            }
            None => {
                self.character_entity.acceleration.y = GRAVITY_Y_ACCEL;
                self.character_entity.contacting_floor = false;
            }
        }

        const MAX_XZ_VELOCITY: f32 = 0.1;
        const XZ_ACCEL: f32 = 0.010;
        const XZ_FRICTION: f32 = 0.004;

        self.character_entity.acceleration.x = 0.;
        self.character_entity.acceleration.x += if self.input_state.is_forward_pressed {
            XZ_ACCEL
        } else if self.character_entity.velocity.x > 0.0 {
            -XZ_FRICTION
        } else {
            0.0
        };
        self.character_entity.acceleration.x += if self.input_state.is_backward_pressed {
            -XZ_ACCEL
        } else if self.character_entity.velocity.x < 0.0 {
            XZ_FRICTION
        } else {
            0.0
        };

        self.character_entity.acceleration.z = 0.;
        self.character_entity.acceleration.z += if self.input_state.is_right_pressed {
            XZ_ACCEL
        } else if self.character_entity.velocity.z > 0.0 {
            -XZ_FRICTION
        } else {
            0.0
        };
        self.character_entity.acceleration.z += if self.input_state.is_left_pressed {
            -XZ_ACCEL
        } else if self.character_entity.velocity.z < 0.0 {
            XZ_FRICTION
        } else {
            0.0
        };

        if self.input_state.jump_button_state == ButtonState::Pressed
            && self.character_entity.contacting_floor
        {
            self.character_entity.acceleration.y += 0.05;
        }

        self.character_entity.velocity += self.character_entity.acceleration;
        self.character_entity.velocity.x = self
            .character_entity
            .velocity
            .x
            .clamp(-MAX_XZ_VELOCITY, MAX_XZ_VELOCITY);
        self.character_entity.velocity.z = self
            .character_entity
            .velocity
            .z
            .clamp(-MAX_XZ_VELOCITY, MAX_XZ_VELOCITY);
        self.character_entity.position += self.character_entity.velocity;

        if (-XZ_FRICTION..XZ_FRICTION).contains(&self.character_entity.velocity.x) {
            self.character_entity.velocity.x = 0.0;
        }
        if (-XZ_FRICTION..XZ_FRICTION).contains(&self.character_entity.velocity.z) {
            self.character_entity.velocity.z = 0.0;
        }
    }

    pub fn process_window_event(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::KeyboardInput { input, .. } => match input.virtual_keycode {
                Some(VirtualKeyCode::I) => {
                    self.input_state.is_forward_pressed = input.state == ElementState::Pressed;
                }
                Some(VirtualKeyCode::J) => {
                    self.input_state.is_left_pressed = input.state == ElementState::Pressed;
                }
                Some(VirtualKeyCode::K) => {
                    self.input_state.is_backward_pressed = input.state == ElementState::Pressed;
                }
                Some(VirtualKeyCode::L) => {
                    self.input_state.is_right_pressed = input.state == ElementState::Pressed;
                }
                Some(VirtualKeyCode::Z) => {
                    let pressed = input.state == ElementState::Pressed;
                    self.input_state.jump_button_state = match pressed {
                        true => match self.input_state.jump_button_state {
                            ButtonState::Pressed => ButtonState::Held,
                            ButtonState::Held => ButtonState::Held,
                            _ => ButtonState::Pressed,
                        },
                        false => match self.input_state.jump_button_state {
                            ButtonState::Pressed => ButtonState::Released,
                            ButtonState::Held => ButtonState::Released,
                            _ => ButtonState::Idle,
                        },
                    }
                }
                _ => (),
            },
            _ => (),
        }
    }
}
