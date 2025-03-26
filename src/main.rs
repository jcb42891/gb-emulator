use std::fs; // For reading files
use log::{info, error};
use std::env;

fn main() {
    env_logger::init();
    let rom_path: Vec<String> = env::args().collect();
    let rom_path = &rom_path[1]; // Filename for ROM (.gb file)
    println!("ROM PATH: {}", rom_path);

    println!("Starting gameboy emulator...");

    match fs::read(rom_path) {
      Ok(rom_data) => {
        info!("Loaded ROM with size: {} bytes", rom_data.len());

        if rom_data.len() < 0x150 {
          error!("ROM too small to have a header!");
          return;
        }

        /*
          The Game Boy ROM has a small section (bytes 0x100–0x14F) with metadata, like:
          Entry point (0x100–0x103): Where the CPU starts running code.
          Title (0x134–0x143): The game’s name (e.g., “POKEMON RED”).
          Cartridge type (0x147): Tells us how memory works.
        */
        // 
        let entry_point = &rom_data[0x100..0x104]; // First 4 bytes
        info!("Entry point: {:02x} {:02x} {:02x} {:02x}", entry_point[0], entry_point[1], entry_point[2], entry_point[3]);

        let title_bytes = &rom_data[0x134..0x144];
        let title_str = String::from_utf8_lossy(title_bytes);
        let title = title_str.trim_end_matches(char::from(0));
        info!("Game title: {}", title);

      },
      Err(e) => error!("Failed to load ROM: {}", e),
    }
}
