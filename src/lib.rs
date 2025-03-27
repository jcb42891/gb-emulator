pub mod cpu;
pub mod ppu;
pub mod memory;

// Re-export frequently used items
pub use ppu::{Ppu, SCREEN_WIDTH, SCREEN_HEIGHT};
pub use cpu::Cpu;
pub use memory::Memory;

// Re-export debug visualization functions
pub use ppu::render_vram_debug_view; 