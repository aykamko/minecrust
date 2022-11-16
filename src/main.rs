#[macro_use]
extern crate itertools;
#[macro_use]
extern crate bmp;

mod camera;
mod face;
mod instance;
mod light;
mod map_generation;
mod spawner;
mod texture;
mod vec_extra;
mod vertex;
mod world;
mod runloop;

fn main() {
    runloop::run();
}