pub mod apu;
pub mod audio;
pub mod bus;
pub mod clock;
pub mod cpu;
pub mod error;
pub mod input;
pub mod loader;
pub mod mapper;
pub mod ppu;
pub mod video;

use crate::loader::Loader;

fn main() {
    let res = Loader::load("roms/super-mario-bros.nes");
    if res.is_err() {
        println!("Error occurs");
    }
}
