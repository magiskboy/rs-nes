//! Cartridge: iNES loader + mapper boards.

mod loader;
pub mod mapper;

pub use loader::{Loader, Mirroring, NesHeader, Rom};
pub use mapper::{mapper_supported, Cartridge};
