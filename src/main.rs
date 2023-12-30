use minecrust::run;
use futures::executor::block_on;

fn main() {
    block_on(run(1024, 1024));
}