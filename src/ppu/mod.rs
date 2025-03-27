use log::info;

pub const SCREEN_WIDTH: usize = 160;
pub const SCREEN_HEIGHT: usize = 144;

pub struct Ppu {
    pub mode: u8,
    pub mode_clock: u32,
    pub line: u8,
    pub vram: Vec<u8>,
    pub oam: Vec<u8>,
    pub frame_buffer: Vec<u8>,
    pub lcdc: u8,
    pub scx: u8,
    pub scy: u8,
    pub bgp: u8,  // Background palette
    pub stat: u8, // LCD status
    pub vblank_interrupt: bool,
    pub wx: u8,   // Window X position
    pub wy: u8,   // Window Y position
    pub obp0: u8,  // Object Palette 0
    pub obp1: u8,  // Object Palette 1
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

    pub fn render_scanline(&mut self) {
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

// A more focused debugging function that shows relevant VRAM data
pub fn render_vram_debug_view(ppu: &mut Ppu) {
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