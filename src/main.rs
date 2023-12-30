use minecrust::run;

#[cfg(not(target_arch = "wasm32"))]
use futures::executor::block_on;

fn main() {
    block_on(run(1024, 1024));
}