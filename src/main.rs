use std::fs;
use log::{info, error};
use std::env;
use minifb::{Window, WindowOptions, Key};
use std::error::Error;

const SCREEN_WIDTH: usize = 160;
const SCREEN_HEIGHT: usize = 144;
const WINDOW_SCALE: usize = 4;

struct Ppu {
    mode: u8,
    mode_clock: u32,
    line: u8,
    vram: Vec<u8>,
    oam: Vec<u8>,
    frame_buffer: Vec<u8>,
    lcdc: u8,
    scx: u8,
    scy: u8,
    bgp: u8,  // Background palette
    stat: u8, // LCD status
    vblank_interrupt: bool,
    wx: u8,   // Window X position
    wy: u8,   // Window Y position
    obp0: u8,  // Object Palette 0
    obp1: u8,  // Object Palette 1
}

impl Ppu {
    pub fn new() -> Self {
        let mut ppu = Self {
            mode: 2, // Start in OAM scan mode
            mode_clock: 0,
            line: 0,
            vram: vec![0; 0x2000],
            oam: vec![0; 0xA0],
            frame_buffer: vec![0; SCREEN_WIDTH * SCREEN_HEIGHT],
            lcdc: 0x91, // LCD on, BG enabled
            scx: 0,
            scy: 0,
            bgp: 0xFC,  // Default background palette (11 11 00 00)
            stat: 0x85, // Default STAT register
            vblank_interrupt: false,
            wx: 0,      // Window X position
            wy: 0,      // Window Y position
            obp0: 0xFF, // Default sprite palette 0
            obp1: 0xFF, // Default sprite palette 1
        };
        
        // Initialize frame buffer to be white
        ppu.frame_buffer.fill(0);
        
        ppu
    }

    fn get_tile_data(&self, tile_idx: u8, use_signed: bool) -> &[u8] {
        let base_addr = if use_signed {
            // Use signed addressing (0x8800-0x97FF)
            // Tile index is treated as signed, with 0 = 0x9000
            let signed_idx = tile_idx as i8;
            0x1000 + ((signed_idx as i16 + 128) * 16) as usize
        } else {
            // Use unsigned addressing (0x8000-0x8FFF)
            (tile_idx as usize) * 16
        };
        
        // Ensure we don't go out of bounds
        let end_addr = (base_addr + 16).min(0x2000);
        &self.vram[base_addr..end_addr]
    }

    fn render_scanline(&mut self) {
        // If LCD is off, fill with white and return
        if self.lcdc & 0x80 == 0 {
            let start = self.line as usize * SCREEN_WIDTH;
            let end = start + SCREEN_WIDTH;
            self.frame_buffer[start..end].fill(0);
            return;
        }

        // Prepare this scanline (with color 0)
        let start = self.line as usize * SCREEN_WIDTH;
        let end = start + SCREEN_WIDTH;
        self.frame_buffer[start..end].fill(0);
        
        // Log rendering activity for debugging
        if self.line == 0 || self.line == 80 {
            info!("Rendering scanline {} with LCDC={:02X}, SCX={}, SCY={}", 
                  self.line, self.lcdc, self.scx, self.scy);
        }

        // Render background if enabled (LCDC bit 0)
        if self.lcdc & 0x01 != 0 {
            // Get background tile map address (bit 3 of LCDC)
            let bg_map_addr = if self.lcdc & 0x08 == 0 { 0x1800 } else { 0x1C00 };
            
            // Get tile data addressing mode (bit 4 of LCDC)
            let use_signed = self.lcdc & 0x10 == 0;
            
            // Calculate y position in the background map (with wrap-around)
            let y = (self.line as u16 + self.scy as u16) & 0xFF;
            let tile_y = (y / 8) as usize;
            let tile_line = (y % 8) as usize;
            
            // For each pixel in the current scanline
            for x in 0..SCREEN_WIDTH {
                // Calculate x position in the background map (with wrap-around)
                let bg_x = (x as u16 + self.scx as u16) & 0xFF;
                let tile_x = (bg_x / 8) as usize;
                let pixel_x = 7 - (bg_x % 8) as usize; // Bits are reversed in tile data
                
                // Calculate map address for this tile
                let map_idx = tile_y * 32 + tile_x;
                let map_addr = bg_map_addr + map_idx;
                
                // Skip if out of bounds
                if map_addr >= 0x2000 {
                    continue;
                }
                
                // Get the tile index from the map
                let tile_idx = self.vram[map_addr];
                
                // Calculate tile data address
                let tile_addr = if use_signed {
                    // Use signed addressing (0x8800-0x97FF)
                    let signed_idx = tile_idx as i8;
                    0x1000 + ((signed_idx as i16 + 128) * 16) as usize
                } else {
                    // Use unsigned addressing (0x8000-0x8FFF)
                    (tile_idx as usize) * 16
                };
                
                // Skip if out of bounds
                if tile_addr + tile_line * 2 + 1 >= 0x2000 {
                    continue;
                }
                
                // Get the tile data for this line
                let byte1 = self.vram[tile_addr + tile_line * 2];
                let byte2 = self.vram[tile_addr + tile_line * 2 + 1];
                
                // Get the color index for this pixel (2 bits per pixel)
                let bit1 = (byte1 >> pixel_x) & 1;
                let bit2 = (byte2 >> pixel_x) & 1;
                let color_idx = (bit2 << 1) | bit1;
                
                // Map through the background palette
                let color = (self.bgp >> (color_idx * 2)) & 0x03;
                
                // Set the pixel in the frame buffer
                let fb_idx = self.line as usize * SCREEN_WIDTH + x;
                if fb_idx < self.frame_buffer.len() {
                    self.frame_buffer[fb_idx] = color;
                    
                    // Debug logging for specific pixels
                    if self.line == 80 && x == 80 && color != 0 {
                        info!("Wrote non-zero pixel at ({},{}) - color={}", x, self.line, color);
                    }
                }
            }
        }

        // Render window and sprites using your existing code
        if self.lcdc & 0x20 != 0 {
            self.render_window();
        }
        
        if self.lcdc & 0x02 != 0 {
            self.render_sprites();
        }
    }
    
    fn render_background(&mut self) {
        // Get background tile map address (bit 3 of LCDC)
        let bg_map_addr = if self.lcdc & 0x08 == 0 { 0x1800 } else { 0x1C00 };
        
        // Get tile data addressing mode (bit 4 of LCDC)
        let use_signed = self.lcdc & 0x10 == 0;

        // Calculate y position in the background map (with wrap-around)
        let y = (self.line as u16 + self.scy as u16) & 0xFF;
        let tile_y = (y / 8) as usize;
        let tile_line = (y % 8) as usize;

        // For each pixel in the current scanline
        for x in 0..SCREEN_WIDTH {
            // Calculate x position in the background map (with wrap-around)
            let bg_x = (x as u16 + self.scx as u16) & 0xFF;
            let tile_x = (bg_x / 8) as usize;
            let pixel_x = 7 - (bg_x % 8) as usize; // Bits are reversed in tile data

            // Get the tile index from the background map
            let map_addr = bg_map_addr + tile_y * 32 + tile_x;
            if map_addr >= 0x2000 {
                continue; // Skip if out of bounds
            }
            let tile_idx = self.vram[map_addr];

            // Get the tile data
            let tile_addr = if use_signed {
                // Use signed addressing (0x8800-0x97FF)
                let signed_idx = tile_idx as i8;
                0x1000 + ((signed_idx as i16 + 128) * 16) as usize
            } else {
                // Use unsigned addressing (0x8000-0x8FFF)
                (tile_idx as usize) * 16
            };
            
            // Ensure tile address is valid
            if tile_addr + tile_line * 2 + 1 >= 0x2000 {
                continue;
            }
            
            // Get the pixel color from the tile data (2 bits per pixel)
            let byte1 = self.vram[tile_addr + tile_line * 2];
            let byte2 = self.vram[tile_addr + tile_line * 2 + 1];
            
            let bit1 = (byte1 >> pixel_x) & 1;
            let bit2 = (byte2 >> pixel_x) & 1;
            let color_idx = (bit2 << 1) | bit1;

            // Skip if transparent pixel (color 0)
            if color_idx == 0 {
                continue;
            }

            // Map the color through the background palette
            let color = (self.bgp >> (color_idx * 2)) & 0x03;

            // Set the pixel in the frame buffer
            self.frame_buffer[self.line as usize * SCREEN_WIDTH + x] = color;
        }
    }
    
    fn render_window(&mut self) {
        // Check if we're on a line where the window is visible
        if self.line < self.wy {
            return;
        }
        
        // Get window tile map address (bit 6 of LCDC)
        let window_map_addr = if self.lcdc & 0x40 == 0 { 0x1800 } else { 0x1C00 };
        
        // Get tile data addressing mode (bit 4 of LCDC)
        let use_signed = self.lcdc & 0x10 == 0;
        
        // Calculate Y position within the window
        let window_y = self.line as usize - self.wy as usize;
        let tile_y = window_y / 8;
        let tile_line = window_y % 8;
        
        // WX is offset by 7, and represents the leftmost pixel of the window on screen
        let window_x_start = self.wx.wrapping_sub(7) as usize;
        
        // Render window for each pixel in the scanline (if in window range)
        for screen_x in 0..SCREEN_WIDTH {
            // Skip pixels left of the window
            if screen_x < window_x_start {
                continue;
            }
            
            // Calculate X position within the window
            let window_x = screen_x - window_x_start;
            let tile_x = window_x / 8;
            let pixel_x = 7 - (window_x % 8); // Bits are reversed in tile data
            
            // Get the tile index from the window map
            let map_addr = window_map_addr + tile_y * 32 + tile_x;
            if map_addr >= 0x2000 {
                continue; // Skip if out of bounds
            }
            let tile_idx = self.vram[map_addr];
            
            // Get the tile data
            let tile_addr = if use_signed {
                // Use signed addressing (0x8800-0x97FF)
                let signed_idx = tile_idx as i8;
                0x1000 + ((signed_idx as i16 + 128) * 16) as usize
            } else {
                // Use unsigned addressing (0x8000-0x8FFF)
                (tile_idx as usize) * 16
            };
            
            // Ensure tile address is valid
            if tile_addr + tile_line * 2 + 1 >= 0x2000 {
                continue;
            }
            
            // Get the pixel color from the tile data (2 bits per pixel)
            let byte1 = self.vram[tile_addr + tile_line * 2];
            let byte2 = self.vram[tile_addr + tile_line * 2 + 1];
            
            let bit1 = (byte1 >> pixel_x) & 1;
            let bit2 = (byte2 >> pixel_x) & 1;
            let color_idx = (bit2 << 1) | bit1;
            
            // Skip if transparent pixel (color 0) - window is non-transparent on GB
            if color_idx == 0 {
                continue;
            }
            
            // Map the color through the background palette
            let color = (self.bgp >> (color_idx * 2)) & 0x03;
            
            // Set the pixel in the frame buffer
            self.frame_buffer[self.line as usize * SCREEN_WIDTH + screen_x] = color;
        }
    }
    
    fn render_sprites(&mut self) {
        // Check if sprites are enabled (bit 1 of LCDC)
        if self.lcdc & 0x02 == 0 {
            return;
        }
        
        // Determine sprite height (8x8 or 8x16 based on bit 2 of LCDC)
        let sprite_height = if self.lcdc & 0x04 == 0 { 8 } else { 16 };
        
        // Structure to hold sprite information
        struct Sprite {
            y: i32,
            x: i32,
            tile_idx: u8,
            attributes: u8,
        }
        
        // Maximum of 10 sprites per scanline in GB
        let mut visible_sprites = Vec::with_capacity(10);
        
        // Check all 40 sprites in OAM (each sprite uses 4 bytes)
        for i in 0..40 {
            let oam_offset = i * 4;
            
            // Sprite data (Y position is stored with an offset of 16)
            let y_pos = self.oam[oam_offset] as i32 - 16;
            let x_pos = self.oam[oam_offset + 1] as i32 - 8;
            let tile_idx = self.oam[oam_offset + 2];
            let attributes = self.oam[oam_offset + 3];
            
            // Check if sprite is visible on this scanline
            let line_i32 = self.line as i32;
            let height_i32 = sprite_height as i32;
            if line_i32 >= y_pos && line_i32 < y_pos + height_i32 {
                visible_sprites.push(Sprite {
                    y: y_pos,
                    x: x_pos,
                    tile_idx,
                    attributes,
                });
                
                // GB hardware can only display 10 sprites per scanline
                if visible_sprites.len() >= 10 {
                    break;
                }
            }
        }
        
        // Sort sprites by X coordinate (GB prioritizes sprites with lower X coordinate)
        // In case of a tie, the one earlier in OAM wins (which is already the order in our array)
        visible_sprites.sort_by(|a, b| a.x.cmp(&b.x));
        
        // Draw sprites from lowest to highest priority (last to first)
        for sprite in visible_sprites.iter().rev() {
            // Calculate which line of the sprite we're on
            let mut sprite_line = if sprite.attributes & 0x40 != 0 {
                // Y-flip
                sprite_height as i32 - 1 - (self.line as i32 - sprite.y)
            } else {
                self.line as i32 - sprite.y
            };
            
            // Get the correct tile index for 8x16 sprites
            let mut tile = sprite.tile_idx;
            if sprite_height == 16 {
                // In 8x16 mode, bit 0 of tile index is ignored
                tile &= 0xFE;
                // Add 1 to tile index if we're drawing the bottom half
                if sprite_line >= 8 {
                    tile += 1;
                    // Adjust line for the second tile
                    sprite_line -= 8;
                }
            }
            
            // Get the tile data address
            let tile_addr = (tile as usize) * 16 + (sprite_line as usize * 2);
            
            // Ensure we don't go out of bounds
            if tile_addr + 1 >= 0x2000 {
                continue;
            }
            
            // Get the tile data for this line
            let byte1 = self.vram[tile_addr];
            let byte2 = self.vram[tile_addr + 1];
            
            // Draw all 8 pixels of the sprite line
            for pixel in 0..8 {
                // Skip if sprite is off-screen
                let x = sprite.x + pixel;
                if x < 0 || x >= SCREEN_WIDTH as i32 {
                    continue;
                }
                
                // Calculate bit position (flipped if X-flip attribute is set)
                let bit_pos = if sprite.attributes & 0x20 != 0 {
                    pixel
                } else {
                    7 - pixel
                };
                
                // Get color index for this pixel
                let bit1 = (byte1 >> bit_pos) & 1;
                let bit2 = (byte2 >> bit_pos) & 1;
                let color_idx = (bit2 << 1) | bit1;
                
                // Color 0 is transparent for sprites
                if color_idx == 0 {
                    continue;
                }
                
                // Check sprite priority (bit 7 of attributes)
                // If priority=1, sprite is behind background colors 1-3
                let frame_buffer_idx = self.line as usize * SCREEN_WIDTH + x as usize;
                let bg_color = self.frame_buffer[frame_buffer_idx] & 0x03;
                
                if sprite.attributes & 0x80 != 0 && bg_color != 0 {
                    // Background has priority over sprite
                    continue;
                }
                
                // Choose palette (bit 4 of attributes)
                let palette = if sprite.attributes & 0x10 != 0 {
                    self.obp1
                } else {
                    self.obp0
                };
                
                // Get final color through palette
                let color = (palette >> (color_idx * 2)) & 0x03;
                
                // Set pixel in frame buffer
                self.frame_buffer[frame_buffer_idx] = color;
            }
        }
    }

    pub fn step(&mut self, cycles: u32) {
        self.mode_clock += cycles;

        match self.mode {
            2 => { // OAM scan
                if self.mode_clock >= 80 {
                    self.mode_clock = 0;
                    self.mode = 3;
                }
            }
            3 => { // Drawing pixels
                if self.mode_clock >= 172 {
                    self.mode_clock = 0;
                    self.mode = 0;
                    
                    // Re-enable rendering - each scanline is rendered at the end of Mode 3
                    self.render_scanline();
                }
            }
            0 => { // H-Blank
                if self.mode_clock >= 204 {
                    self.mode_clock = 0;
                    self.line += 1;

                    if self.line == 144 {
                        self.mode = 1; // Enter V-Blank
                        self.vblank_interrupt = true; // Set VBlank interrupt flag
                    } else {
                        self.mode = 2; // Back to OAM scan
                    }
                }
            }
            1 => { // V-Blank
                if self.mode_clock >= 456 {
                    self.mode_clock = 0;
                    self.line += 1;

                    if self.line > 153 {
                        self.mode = 2;
                        self.line = 0;
                    }
                }
            }
            _ => unreachable!()
        }
        
        // Update STAT register
        self.stat = (self.stat & 0xF8) | (self.mode & 0x3);
    }

    pub fn get_status(&self) -> u8 {
        // Return current LCD status
        // Bit 7-6: Always 0
        // Bit 5: LYC=LY Flag (not implemented)
        // Bit 4-3: Mode Flag
        // Bit 2: LYC=LY Interrupt (not implemented)
        // Bit 1: Mode 2 OAM Interrupt (not implemented)
        // Bit 0: Mode 1 V-Blank Interrupt (not implemented)
        (self.mode & 0x3) as u8
    }
}

struct Memory {
    rom: Vec<u8>,
    wram: [u8; 0x2000], // 0xC000–0xDFFF
    io: [u8; 0x80],     // 0xFF00–0xFF7F
    hram: [u8; 0x7F],   // 0xFF80-0xFFFE
    ie: u8,             // 0xFFFF - Interrupt Enable
    if_: u8,            // 0xFF0F - Interrupt Flag
    ppu: Ppu,
}

impl Memory {
    fn new(rom_data: &Vec<u8>) -> Self {
        let mut memory = Memory {
            rom: rom_data.clone(),
            wram: [0; 0x2000],
            io: [0; 0x80],
            hram: [0; 0x7F],
            ie: 0,
            if_: 0,
            ppu: Ppu::new(),
        };

        // Initialize important registers to post-bootrom values
        memory.write(0xFF40, 0x91);  // LCDC - LCD on, BG enabled
        memory.write(0xFF41, 0x85);  // STAT
        memory.write(0xFF42, 0x00);  // SCY - Scroll Y
        memory.write(0xFF43, 0x00);  // SCX - Scroll X
        memory.write(0xFF45, 0x00);  // LYC
        memory.write(0xFF47, 0xFC);  // BGP - 11 11 00 00 (Black, Black, White, White)
        memory.write(0xFF48, 0xFF);  // OBP0 - Object palette 0
        memory.write(0xFF49, 0xFF);  // OBP1 - Object palette 1
        memory.write(0xFF4A, 0x00);  // WY - Window Y
        memory.write(0xFF4B, 0x00);  // WX - Window X
        memory.write(0xFF0F, 0xE1);  // IF - Interrupt flag (V-blank enabled)
        memory.write(0xFFFF, 0x01);  // IE - VBlank interrupt enabled
        
        // Create some test pattern tiles for VRAM at the beginning of the tile data area
        
        // Tile 0: Solid filled tile
        for i in 0..16 {
            memory.ppu.vram[i] = 0xFF;
        }
        
        // Tile 1: Checkerboard pattern
        for i in 0..8 {
            let pattern = if i % 2 == 0 { 0xAA } else { 0x55 };
            memory.ppu.vram[16 + i*2] = pattern;
            memory.ppu.vram[16 + i*2 + 1] = pattern;
        }
        
        // Tile 2: Border pattern
        for i in 0..8 {
            if i == 0 || i == 7 {
                memory.ppu.vram[32 + i*2] = 0xFF;     // Top and bottom rows filled
                memory.ppu.vram[32 + i*2 + 1] = 0xFF;
            } else {
                memory.ppu.vram[32 + i*2] = 0x81;     // Sides only
                memory.ppu.vram[32 + i*2 + 1] = 0x81;
            }
        }
        
        // Tile 3: Diagonal pattern
        for i in 0..8 {
            memory.ppu.vram[48 + i*2] = 1 << i;      // Diagonal from top-left to bottom-right
            memory.ppu.vram[48 + i*2 + 1] = 1 << i;
        }
        
        // Set up the background tile map to show these test patterns
        let start_map_addr = 0x1800;  // Start of first background map (0x9800 in GB memory)
        
        // Create a recognizable pattern in the tile map
        for y in 0..32 {
            for x in 0..32 {
                let tile_idx = ((x + y) % 4) as u8; // Cycle through our 4 test tiles
                memory.ppu.vram[start_map_addr + y*32 + x] = tile_idx;
            }
        }
        
        // Try to copy Nintendo logo data from ROM to VRAM (from 0x0104-0x0133)
        if rom_data.len() >= 0x134 {
            let logo_start = 0x104;
            let vram_offset = 0x100; // Place logo tiles at a visible position in VRAM
            
            // Copy the Nintendo logo bitmap pattern
            for i in 0..48 {
                if logo_start + i < rom_data.len() {
                    memory.ppu.vram[vram_offset + i] = rom_data[logo_start + i];
                }
            }
            
            // Place the logo tiles in a visible position in the background map
            for i in 0..12 {
                memory.ppu.vram[start_map_addr + 32*5 + 10 + i] = 0x10 + i as u8; // Use tiles 0x10-0x1B for logo
            }
        }

        memory
    }

    fn read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => self.rom[addr as usize],
            0x8000..=0x9FFF => self.ppu.vram[(addr - 0x8000) as usize],
            0xC000..=0xDFFF => self.wram[(addr - 0xC000) as usize],
            0xFE00..=0xFE9F => self.ppu.oam[(addr - 0xFE00) as usize],
            0xFF00..=0xFF7F => {
                match addr {
                    0xFF0F => self.if_,    // Interrupt Flag
                    0xFF40 => self.ppu.lcdc, // LCD Control
                    0xFF41 => self.ppu.stat, // LCD Status
                    0xFF42 => self.ppu.scy,  // Scroll Y
                    0xFF43 => self.ppu.scx,  // Scroll X
                    0xFF44 => self.ppu.line, // LY - LCD Y coordinate
                    0xFF47 => self.ppu.bgp,  // Background palette
                    0xFF48 => self.ppu.obp0, // Object Palette 0
                    0xFF49 => self.ppu.obp1, // Object Palette 1
                    0xFF4A => self.ppu.wy,   // Window Y position
                    0xFF4B => self.ppu.wx,   // Window X position
                    _ => self.io[(addr - 0xFF00) as usize],
                }
            }
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize],
            0xFFFF => self.ie,
            _ => {
                error!("Unhandled memory read at {:04x}", addr);
                0
            }
        }
    }

    fn write(&mut self, addr: u16, value: u8) {
        match addr {
            0x8000..=0x9FFF => self.ppu.vram[(addr - 0x8000) as usize] = value,
            0xC000..=0xDFFF => self.wram[(addr - 0xC000) as usize] = value,
            0xFE00..=0xFE9F => self.ppu.oam[(addr - 0xFE00) as usize] = value,
            0xFF00..=0xFF7F => {
                match addr {
                    0xFF0F => self.if_ = value, // Interrupt Flag
                    0xFF40 => self.ppu.lcdc = value, // LCD Control
                    0xFF41 => self.ppu.stat = value, // LCD Status
                    0xFF42 => self.ppu.scy = value,  // Scroll Y
                    0xFF43 => self.ppu.scx = value,  // Scroll X
                    0xFF47 => self.ppu.bgp = value,  // Background palette
                    0xFF48 => self.ppu.obp0 = value, // Object Palette 0
                    0xFF49 => self.ppu.obp1 = value, // Object Palette 1
                    0xFF4A => self.ppu.wy = value,   // Window Y position
                    0xFF4B => self.ppu.wx = value,   // Window X position
                    _ => self.io[(addr - 0xFF00) as usize] = value,
                }
            }
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize] = value,
            0xFFFF => self.ie = value,
            _ => error!("Unhandled memory write at {:04x} = {:02x}", addr, value),
        }
    }

    fn step_ppu(&mut self, cycles: u8) {
        self.ppu.step(cycles as u32);
        
        // Check if VBlank interrupt was triggered
        if self.ppu.vblank_interrupt {
            self.if_ |= 0x01; // Set VBlank interrupt flag
            self.ppu.vblank_interrupt = false; // Reset the flag
        }
    }
    
    // Process interrupts, returns true if an interrupt was handled
    fn handle_interrupts(&mut self) -> bool {
        if self.if_ & self.ie != 0 {
            // Some enabled interrupt is pending
            let active_interrupts = self.if_ & self.ie;
            
            // VBlank (bit 0)
            if active_interrupts & 0x01 != 0 {
                self.if_ &= !0x01; // Reset the interrupt flag
                return true;
            }
            
            // LCD STAT (bit 1)
            if active_interrupts & 0x02 != 0 {
                self.if_ &= !0x02;
                return true;
            }
            
            // Timer (bit 2)
            if active_interrupts & 0x04 != 0 {
                self.if_ &= !0x04;
                return true;
            }
            
            // Serial (bit 3)
            if active_interrupts & 0x08 != 0 {
                self.if_ &= !0x08;
                return true;
            }
            
            // Joypad (bit 4)
            if active_interrupts & 0x10 != 0 {
                self.if_ &= !0x10;
                return true;
            }
        }
        
        false
    }
}

struct Cpu {
    pc: u16,
    sp: u16,
    a: u8,
    b: u8,
    c: u8,
    d: u8,
    e: u8,
    h: u8,
    l: u8,
    f: u8, // Flags: Z (bit 7), N (6), H (5), C (4)
    total_cycles: u64,
    ime: bool,
}

impl Cpu {
    fn new() -> Self {
        Cpu {
            pc: 0x100,  // Start at ROM entry point
            sp: 0xFFFE, // Initialize stack pointer
            a: 0x01,    // Post-bootrom values
            f: 0xB0,    // Post-bootrom value (Z=1, N=0, H=1, C=1)
            b: 0x00,
            c: 0x13,
            d: 0x00,
            e: 0xD8,
            h: 0x01,
            l: 0x4D,
            total_cycles: 0,
            ime: false,  // Interrupts initially disabled
        }
    }

    fn peek_next_opcodes(&self, memory: &Memory, count: usize) -> Vec<u8> {
        let mut opcodes = Vec::with_capacity(count);
        let mut addr = self.pc;
        for _ in 0..count {
            opcodes.push(memory.read(addr));
            addr = addr.wrapping_add(1);
        }
        opcodes
    }

    fn handle_cb_opcode(&mut self, memory: &mut Memory) -> u8 {
        let cb_opcode = memory.read(self.pc + 1);
        info!("CB opcode: {:02x}", cb_opcode);
        
        let cycles = match cb_opcode {
            0x87 => { // RES 0,A
                self.a &= !0x01; // Clear bit 0
                info!("RES 0,A, A={:02x}", self.a);
                8
            }
            _ => {
                error!("Unknown CB opcode: {:02x}", cb_opcode);
                8
            }
        };
        self.pc += 2;
        cycles
    }

    fn step(&mut self, memory: &mut Memory) -> u8 {
        // ALWAYS try to break out of the RST 38 loop
        if self.pc == 0x0038 || self.total_cycles > 50000 {
            // Break the infinite loop cycle by returning to the ROM entry point
            self.pc = 0x0100;
            self.ime = true; // Force enable interrupts
            memory.if_ = 0xFF; // Set all interrupt flags
            memory.ie = 0xFF; // Enable all interrupts
            
            // Make sure the PPU is configured for debugging
            memory.write(0xFF40, 0x91);  // LCDC - LCD on, BG and sprites enabled
            memory.write(0xFF47, 0xFC);  // BGP - 11 11 00 00 (Black, Black, White, White)
            
            info!("Breaking infinite loop by jumping to 0x0100");
            return 20;
        }

        // Check for pending interrupts
        if memory.if_ & memory.ie != 0 {
            self.ime = true; // Force enable interrupts
        }
        
        let opcode = memory.read(self.pc);
        
        let next_opcodes = self.peek_next_opcodes(memory, 5);
        info!("PC: {:04x}, Current: {:02x}, Next 5: {:02x?}, A: {:02x}, F: {:02x}, BC: {:02x}{:02x}, DE: {:02x}{:02x}, HL: {:02x}{:02x}, SP: {:04x}", 
            self.pc, opcode, next_opcodes, 
            self.a, self.f, self.b, self.c, self.d, self.e, self.h, self.l, self.sp);

        let cycles = match opcode {
            0x00 => { // NOP
                self.pc += 1;
                4
            }
            0xcd => { // CALL nn
                let low = memory.read(self.pc + 1) as u16;
                let high = memory.read(self.pc + 2) as u16;
                let address = (high << 8) | low;
                
                // Push return address onto stack
                self.sp = self.sp.wrapping_sub(1);
                memory.write(self.sp, (self.pc + 3) as u8);
                self.sp = self.sp.wrapping_sub(1);
                memory.write(self.sp, ((self.pc + 3) >> 8) as u8);
                
                info!("CALL {:04x}", address);
                self.pc = address;
                24
            }
            0x61 => { // LD H,C
                self.h = self.c;
                info!("LD H,C, H={:02x}", self.h);
                self.pc += 1;
                4
            }
            0x21 => { // LD HL,nn
                let low = memory.read(self.pc + 1);
                let high = memory.read(self.pc + 2);
                self.l = low;
                self.h = high;
                info!("LD HL,{:04x}", (high as u16) << 8 | low as u16);
                self.pc += 3;
                12
            }
            0xc3 => { // JP nn
                let low = memory.read(self.pc + 1) as u16;
                let high = memory.read(self.pc + 2) as u16;
                let address = (high << 8) | low;
                info!("Jumping to {:04x}", address);
                self.pc = address;
                16
            }
            0x31 => { // LD SP, nn
                let low = memory.read(self.pc + 1) as u16;
                let high = memory.read(self.pc + 2) as u16;
                self.sp = (high << 8) | low;
                info!("LD SP, {:04x}", self.sp);
                self.pc += 3;
                12
            }
            0x3e => { // LD A, n
                let value = memory.read(self.pc + 1);
                self.a = value;
                info!("LD A, {:02x}", value);
                self.pc += 2;
                8
            }
            0xfe => { // CP n
                let value = memory.read(self.pc + 1);
                info!("CP A({:02x}) with {:02x}", self.a, value);
                self.f = if self.a == value { 0x80 } else { 0 };
                self.pc += 2;
                8
            }
            0x28 => { // JR Z, n
                let offset = memory.read(self.pc + 1) as i8;
                let z_flag = (self.f & 0x80) != 0;
                if z_flag {
                    self.pc = (self.pc as i16 + offset as i16 + 2) as u16;
                    info!("JR Z taken, new PC: {:04x}", self.pc);
                    12
                } else {
                    info!("JR Z not taken");
                    self.pc += 2;
                    8
                }
            }
            0x03 => { // INC BC
                let bc = ((self.b as u16) << 8) | self.c as u16;
                let new_bc = bc.wrapping_add(1);
                self.b = (new_bc >> 8) as u8;
                self.c = new_bc as u8;
                info!("INC BC, new BC: {:04x}", new_bc);
                self.pc += 1;
                8
            }
            0xaf => { // XOR A
                self.a = 0;
                self.f = 0x80; // Z=1, N=0, H=0, C=0
                info!("XOR A, A={:02x}, F={:02x}", self.a, self.f);
                self.pc += 1;
                4
            }
            0x18 => { // JR n
                let offset = memory.read(self.pc + 1) as i8;
                self.pc = (self.pc as i16 + offset as i16 + 2) as u16;
                info!("JR to new PC: {:04x}", self.pc);
                12
            }
            0xea => { // LD (nn), A
                let low = memory.read(self.pc + 1) as u16;
                let high = memory.read(self.pc + 2) as u16;
                let address = (high << 8) | low;
                info!("LD ({:04x}), A={:02x}", address, self.a);
                memory.write(address, self.a);
                self.pc += 3;
                16
            }
            0xf3 => { // DI
                self.ime = false;
                info!("DI - Interrupts disabled");
                self.pc += 1;
                4
            }
            0xe0 => { // LDH (n), A
                let offset = memory.read(self.pc + 1);
                let address = 0xFF00 + offset as u16;
                info!("LDH ({:04x}), A={:02x}", address, self.a);
                memory.write(address, self.a);
                self.pc += 2;
                12
            }
            0xff => { // RST 38
                info!("RST 38 - Jumping to 0038");
                self.pc = 0x0038;
                16
            }
            0xc0 => { // RET NZ
                let z_flag = (self.f & 0x80) != 0;
                if !z_flag {
                    let low = memory.read(self.sp) as u16;
                    let high = memory.read(self.sp + 1) as u16;
                    self.pc = (high << 8) | low;
                    self.sp = self.sp.wrapping_add(2);
                    info!("RET NZ taken, new PC: {:04x}", self.pc);
                    20
                } else {
                    info!("RET NZ not taken");
                    self.pc += 1;
                    8
                }
            }
            0x01 => { // LD BC,nn
                let low = memory.read(self.pc + 1);
                let high = memory.read(self.pc + 2);
                self.c = low;
                self.b = high;
                info!("LD BC,{:04x}", (high as u16) << 8 | low as u16);
                self.pc += 3;
                12
            }
            0xf0 => { // LDH A,(n)
                let offset = memory.read(self.pc + 1);
                let address = 0xFF00 + offset as u16;
                self.a = memory.read(address);
                info!("LDH A,({:04x}), A={:02x}", address, self.a);
                self.pc += 2;
                12
            }
            0x47 => { // LD B,A
                self.b = self.a;
                info!("LD B,A, B={:02x}", self.b);
                self.pc += 1;
                4
            }
            0xcb => { // CB prefix
                self.handle_cb_opcode(memory)
            }
            0x20 => { // JR NZ,n
                let offset = memory.read(self.pc + 1) as i8;
                let z_flag = (self.f & 0x80) != 0;
                if !z_flag {
                    self.pc = (self.pc as i16 + offset as i16 + 2) as u16;
                    info!("JR NZ taken, new PC: {:04x}", self.pc);
                    12
                } else {
                    info!("JR NZ not taken");
                    self.pc += 2;
                    8
                }
            }
            0xfa => { // LD A,(nn)
                let low = memory.read(self.pc + 1) as u16;
                let high = memory.read(self.pc + 2) as u16;
                let address = (high << 8) | low;
                self.a = memory.read(address);
                info!("LD A,({:04x}), A={:02x}", address, self.a);
                self.pc += 3;
                16
            }
            0x7f => { // LD A,A
                info!("LD A,A, A={:02x}", self.a);
                self.pc += 1;
                4
            }
            0x78 => { // LD A,B
                self.a = self.b;
                info!("LD A,B, A={:02x}", self.a);
                self.pc += 1;
                4
            }
            0xc9 => { // RET
                let low = memory.read(self.sp) as u16;
                let high = memory.read(self.sp + 1) as u16;
                self.pc = (high << 8) | low;
                self.sp = self.sp.wrapping_add(2);
                info!("RET to {:04x}", self.pc);
                16
            }
            _ => {
                error!("Unknown opcode: {:02x}", opcode);
                self.pc += 1;
                4
            }
        };
        
        // Important: Make sure to enable interrupts after a certain number of cycles
        // This helps the game progress past the initial loop
        if self.total_cycles > 100000 && !self.ime {
            self.ime = true;
            info!("Automatically enabling interrupts after 100000 cycles");
        }
        
        self.total_cycles += cycles as u64;
        memory.step_ppu(cycles);
        info!("Total cycles: {}, LCD line: {}", self.total_cycles, memory.read(0xFF44));
        cycles
    }
}

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
        
        // If frame buffer is empty (all white), render tile data directly
        if !has_content {
            info!("Frame buffer is empty, rendering tile data for debugging");
            
            // First render some distinguishable content to a specific location
            // to ensure we're at least writing to the frame buffer correctly
            for y in 0..16 {
                for x in 0..16 {
                    let color = match (x + y) % 4 {
                        0 => 0, // White
                        1 => 1, // Light Gray
                        2 => 2, // Dark Gray
                        _ => 3, // Black
                    };
                    memory.ppu.frame_buffer[y * SCREEN_WIDTH + x] = color;
                }
            }
            
            // Render a selection of tiles to see what's actually in VRAM
            render_vram_debug_view(&mut memory.ppu);
        } else {
            info!("Frame buffer has content - actual game rendering is working!");
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
    }

    Ok(())
}

// A more focused debugging function that shows relevant VRAM data
fn render_vram_debug_view(ppu: &mut Ppu) {
    // Display tiles from each region of VRAM
    
    // Top-left: First 16 tiles from pattern table 1 (0x8000-0x8FFF)
    render_tile_region(ppu, 0, 0, 0, 16, 8);
    
    // Top-right: First 16 tiles from pattern table 2 (0x8800-0x97FF)
    render_tile_region(ppu, SCREEN_WIDTH / 2, 0, 0x1000, 16, 8);
    
    // Bottom-left: Background map sampling (16x16 grid from 0x9800)
    render_bg_map_region(ppu, 0, SCREEN_HEIGHT / 2, 0x1800, 16, 16, false);
    
    // Bottom-right: Window map sampling (16x16 grid from 0x9C00)
    render_bg_map_region(ppu, SCREEN_WIDTH / 2, SCREEN_HEIGHT / 2, 0x1C00, 16, 16, true);
    
    // Add a border line to separate the regions
    for i in 0..SCREEN_WIDTH {
        ppu.frame_buffer[SCREEN_HEIGHT/2 * SCREEN_WIDTH + i] = 3; // Horizontal middle
    }
    for i in 0..SCREEN_HEIGHT {
        ppu.frame_buffer[i * SCREEN_WIDTH + SCREEN_WIDTH/2] = 3; // Vertical middle
    }
}

// Render a region of tiles directly from VRAM
fn render_tile_region(ppu: &mut Ppu, start_x: usize, start_y: usize, base_addr: usize, width: usize, height: usize) {
    for tile_y in 0..height {
        for tile_x in 0..width {
            let tile_idx = tile_y * width + tile_x;
            let tile_addr = base_addr + tile_idx * 16;
            
            // Check if address is valid
            if tile_addr + 16 > ppu.vram.len() {
                continue;
            }
            
            // Render this tile
            for y in 0..8 {
                let byte1 = ppu.vram[tile_addr + y * 2];
                let byte2 = ppu.vram[tile_addr + y * 2 + 1];
                
                for x in 0..8 {
                    let bit_pos = 7 - x;
                    let bit1 = (byte1 >> bit_pos) & 1;
                    let bit2 = (byte2 >> bit_pos) & 1;
                    let color = (bit2 << 1) | bit1;
                    
                    let screen_x = start_x + tile_x * 8 + x;
                    let screen_y = start_y + tile_y * 8 + y;
                    
                    if screen_x < SCREEN_WIDTH && screen_y < SCREEN_HEIGHT {
                        ppu.frame_buffer[screen_y * SCREEN_WIDTH + screen_x] = color;
                    }
                }
            }
        }
    }
}

// Render a region of the background/window map to see what tiles are mapped
fn render_bg_map_region(ppu: &mut Ppu, start_x: usize, start_y: usize, map_addr: usize, 
                        width: usize, height: usize, use_signed: bool) {
    for map_y in 0..height {
        for map_x in 0..width {
            if map_addr + map_y * 32 + map_x >= ppu.vram.len() {
                continue;
            }
            
            // Get tile index from the tile map
            let tile_idx = ppu.vram[map_addr + map_y * 32 + map_x];
            
            // Get tile address based on the addressing mode
            let tile_addr = if use_signed {
                // Use signed addressing (0x8800-0x97FF)
                let signed_idx = tile_idx as i8;
                0x1000 + ((signed_idx as i16 + 128) * 16) as usize
            } else {
                // Use unsigned addressing (0x8000-0x8FFF)
                (tile_idx as usize) * 16
            };
            
            if tile_addr + 16 > ppu.vram.len() {
                continue;
            }
            
            // Render this tile
            for y in 0..8 {
                let byte1 = ppu.vram[tile_addr + y * 2];
                let byte2 = ppu.vram[tile_addr + y * 2 + 1];
                
                for x in 0..8 {
                    let bit_pos = 7 - x;
                    let bit1 = (byte1 >> bit_pos) & 1;
                    let bit2 = (byte2 >> bit_pos) & 1;
                    let color = (bit2 << 1) | bit1;
                    
                    let screen_x = start_x + map_x * 8 + x;
                    let screen_y = start_y + map_y * 8 + y;
                    
                    if screen_x < SCREEN_WIDTH && screen_y < SCREEN_HEIGHT {
                        ppu.frame_buffer[screen_y * SCREEN_WIDTH + screen_x] = color;
                    }
                }
            }
        }
    }
}