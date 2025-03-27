use std::fs;
use log::{info, error};
use std::env;
use minifb::{Window, WindowOptions, Key};
use std::error::Error;

// Import from our crate modules
use gb_emulator::{Cpu, Memory, SCREEN_WIDTH, SCREEN_HEIGHT, render_vram_debug_view};

const WINDOW_SCALE: usize = 4;

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <rom_file>", args[0]);
        std::process::exit(1);
    }

    let rom_path = &args[1];
    info!("Loading ROM from {}", rom_path);
    let rom_data = fs::read(rom_path)?;

    info!("ROM entry point: {:02X}", rom_data[0x100]);
    info!("Game title: {}", String::from_utf8_lossy(&rom_data[0x134..0x144]));
    info!("Cartridge type: {:02X}", rom_data[0x147]);
    info!("ROM size: {:02X}", rom_data[0x148]);
    info!("RAM size: {:02X}", rom_data[0x149]);

    let mut memory = Memory::new(&rom_data);
    let mut cpu = Cpu::new();

    // Configure PPU for optimal initial state
    memory.ppu.lcdc = 0x91; // LCD on, BG enabled
    memory.ppu.scy = 0;     // Initial scroll Y
    memory.ppu.scx = 0;     // Initial scroll X
    memory.ppu.bgp = 0xE4;  // Standard Game Boy palette

    let mut window = Window::new(
        "Game Boy Emulator",
        SCREEN_WIDTH * WINDOW_SCALE,
        SCREEN_HEIGHT * WINDOW_SCALE,
        WindowOptions::default(),
    )?;

    // Buffer to store the scaled ARGB pixels
    let mut buffer = vec![0u32; SCREEN_WIDTH * WINDOW_SCALE * SCREEN_HEIGHT * WINDOW_SCALE];

    // Game Boy DMG colors - White, Light Gray, Dark Gray, Black (classic palette)
    let palette = [0xFFFFFFFF, 0xFFAAAAAA, 0xFF555555, 0xFF000000];

    // Configure display mode - set to true for debug overlay, false for normal rendering
    let mut debug_mode = false;

    // Main game loop
    while window.is_open() && !window.is_key_down(Key::Escape) {
        // Run CPU for one frame (70224 cycles)
        let mut frame_cycles = 0;
        while frame_cycles < 70224 {
            let cycles = cpu.step(&mut memory);
            frame_cycles += cycles as u32;
        }
        
        // Log PPU state for debugging
        info!("PPU State - LCDC: {:02X}, BG Palette: {:02X}, SCX: {}, SCY: {}", 
              memory.ppu.lcdc, memory.ppu.bgp, memory.ppu.scx, memory.ppu.scy);
        
        // Check if frame buffer has any non-zero pixels (actual content)
        let has_content = memory.ppu.frame_buffer.iter().any(|&pixel| pixel != 0);
        
        // Enable debug mode visualization if there's no content or if debug mode is active
        if !has_content || debug_mode {
            info!("Rendering debug view");
            render_vram_debug_view(&mut memory.ppu);
        }

        // Convert Game Boy colors to ARGB and scale
        for y in 0..SCREEN_HEIGHT {
            for x in 0..SCREEN_WIDTH {
                let color_idx = memory.ppu.frame_buffer[y * SCREEN_WIDTH + x] as usize;
                let argb = palette[color_idx & 0x3]; // Ensure we stay in bounds

                // Scale the pixel
                for sy in 0..WINDOW_SCALE {
                    for sx in 0..WINDOW_SCALE {
                        let buffer_idx = (y * WINDOW_SCALE + sy) * (SCREEN_WIDTH * WINDOW_SCALE) + (x * WINDOW_SCALE + sx);
                        buffer[buffer_idx] = argb;
                    }
                }
            }
        }

        // Update the window with the scaled buffer
        if let Err(e) = window.update_with_buffer(&buffer, SCREEN_WIDTH * WINDOW_SCALE, SCREEN_HEIGHT * WINDOW_SCALE) {
            error!("Failed to update window: {}", e);
        }

        // Toggle debug mode with D key
        if window.is_key_pressed(Key::D, minifb::KeyRepeat::No) {
            debug_mode = !debug_mode;
            info!("Debug mode: {}", debug_mode);
        }
    }

    Ok(())
}