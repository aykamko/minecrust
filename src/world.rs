struct Block {
    // TODO: should be an enum
    block_type: u8,
}

const WORLD_XZ_SIZE: usize = 846;
const WORLD_Y_SIZE: usize = 256;

impl Default for Block {
    fn default() -> Block {
        Block {
            block_type: 0,
        }
    }
}

pub struct WorldState {
    blocks: [[[Block; WORLD_XZ_SIZE]; WORLD_XZ_SIZE]; WORLD_Y_SIZE],
}

impl WorldState {
    pub fn new() -> Self {
        Self {
            blocks: [[[Block { block_type: 0 }; WORLD_XZ_SIZE]; WORLD_XZ_SIZE]; WORLD_Y_SIZE],
        }
    }
}

pub fn setup_world(world_state: &mut WorldState) {
    world_state.blocks[0][0][0].block_type = 1;
}
