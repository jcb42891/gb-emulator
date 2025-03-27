use log::{info, error};
use crate::ppu::Ppu;

pub struct Memory {
    pub rom: Vec<u8>,
    pub wram: [u8; 0x2000], // 0xC000–0xDFFF
    pub io: [u8; 0x80],     // 0xFF00–0xFF7F
    pub hram: [u8; 0x7F],   // 0xFF80-0xFFFE
    pub ie: u8,             // 0xFFFF - Interrupt Enable
    pub if_: u8,            // 0xFF0F - Interrupt Flag
    pub ppu: Ppu,
}

impl Memory {
    pub fn new(rom_data: &Vec<u8>) -> Self {
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

    pub fn read(&self, addr: u16) -> u8 {
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

    pub fn write(&mut self, addr: u16, value: u8) {
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

    pub fn step_ppu(&mut self, cycles: u8) {
        self.ppu.step(cycles as u32);
        
        // Check if VBlank interrupt was triggered
        if self.ppu.vblank_interrupt {
            self.if_ |= 0x01; // Set VBlank interrupt flag
            self.ppu.vblank_interrupt = false; // Reset the flag
        }
    }
    
    // Process interrupts, returns true if an interrupt was handled
    pub fn handle_interrupts(&mut self) -> bool {
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