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
            bgp: 0xE4,  // Default background palette (11 10 01 00)
            stat: 0x85, // Default STAT register
            vblank_interrupt: false,
        };
        
        // Initialize frame buffer with a visible test pattern
        for y in 0..SCREEN_HEIGHT {
            for x in 0..SCREEN_WIDTH {
                let color = ((x / 8 + y / 8) % 4) as u8;  // Create a simple checkerboard
                ppu.frame_buffer[y * SCREEN_WIDTH + x] = color;
            }
        }
        
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
        // Clear the scanline first
        let start = self.line as usize * SCREEN_WIDTH;
        let end = start + SCREEN_WIDTH;
        
        if self.lcdc & 0x80 == 0 {
            // LCD is off - fill with white
            self.frame_buffer[start..end].fill(0);
            return;
        }

        // Initialize with white
        self.frame_buffer[start..end].fill(0);

        // Check if background is enabled
        if self.lcdc & 0x01 == 0 {
            // Background disabled, leave as white
            return;
        }

        // Get background tile map address (0x9800 or 0x9C00)
        let bg_map_addr = if self.lcdc & 0x08 == 0 { 0x1800 } else { 0x1C00 };
        
        // Get tile data addressing mode
        // Bit 4: 0=8800-97FF, 1=8000-8FFF
        let use_signed = self.lcdc & 0x10 == 0;

        // Calculate y position in the background map
        let y = (self.line as u16 + self.scy as u16) & 0xFF;
        let tile_y = (y / 8) as usize;
        let tile_line = (y % 8) as usize;

        // Render the scanline
        for x in 0..SCREEN_WIDTH {
            // Calculate x position in the background map
            let bg_x = (x as u16 + self.scx as u16) & 0xFF;
            let tile_x = (bg_x / 8) as usize;
            let pixel_x = 7 - (bg_x % 8) as usize; // Bits are reversed

            // Get the tile index from the background map
            let map_addr = bg_map_addr + tile_y * 32 + tile_x;
            if map_addr >= 0x2000 {
                continue; // Skip if out of bounds
            }
            let tile_idx = self.vram[map_addr];

            // Get the tile data
            let tile_addr = if use_signed {
                // Use signed addressing (0x8800-0x97FF)
                // Tile index is treated as signed, with 0 = 0x9000
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

            // Map the color through the background palette (0=White, 1=Light Gray, 2=Dark Gray, 3=Black)
            let color = (self.bgp >> (color_idx * 2)) & 0x03;

            // Set the pixel in the frame buffer
            self.frame_buffer[self.line as usize * SCREEN_WIDTH + x] = color;
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
                    // Skip render_scanline for now, using our static test pattern
                    // self.render_scanline();
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
        // These will not affect our test pattern that's already in the frame buffer
        memory.write(0xFF40, 0x91);  // LCDC - LCD on, BG enabled
        memory.write(0xFF41, 0x85);  // STAT
        memory.write(0xFF42, 0x00);  // SCY - Scroll Y
        memory.write(0xFF43, 0x00);  // SCX - Scroll X
        memory.write(0xFF45, 0x00);  // LYC
        memory.write(0xFF47, 0xE4);  // BGP - 11 10 01 00 (darker to lighter)
        memory.write(0xFF48, 0xFF);  // OBP0 - Object palette 0
        memory.write(0xFF49, 0xFF);  // OBP1 - Object palette 1
        memory.write(0xFF4A, 0x00);  // WY - Window Y
        memory.write(0xFF4B, 0x00);  // WX - Window X
        memory.write(0xFF0F, 0xE1);  // IF - Interrupt flag (V-blank enabled)
        memory.write(0xFFFF, 0x01);  // IE - VBlank interrupt enabled

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
        if self.pc == 0x0038 {
            // Break the infinite loop cycle by returning to the ROM entry point
            self.pc = 0x0100;
            self.ime = true; // Force enable interrupts
            memory.if_ = 0xFF; // Set all interrupt flags
            memory.ie = 0xFF; // Enable all interrupts
            info!("Breaking infinite RST 38 loop by jumping to 0x0100");
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

    let mut window = Window::new(
        "Game Boy Emulator",
        SCREEN_WIDTH * WINDOW_SCALE,
        SCREEN_HEIGHT * WINDOW_SCALE,
        WindowOptions::default(),
    )?;

    // Buffer to store the scaled ARGB pixels
    let mut buffer = vec![0u32; SCREEN_WIDTH * WINDOW_SCALE * SCREEN_HEIGHT * WINDOW_SCALE];

    // Gameboy DMG colors - White, Light Gray, Dark Gray, Black
    let palette = [0xFFFFFFFF, 0xFFAAAAAA, 0xFF555555, 0xFF000000];

    // Main game loop
    while window.is_open() && !window.is_key_down(Key::Escape) {
        // Run CPU for one frame (70224 cycles)
        let mut frame_cycles = 0;
        while frame_cycles < 70224 {
            let cycles = cpu.step(&mut memory);
            frame_cycles += cycles as u32;
        }

        // Debug: Print first few pixels to check if they're being set
        info!("First 10 pixels of frame buffer: {:?}", &memory.ppu.frame_buffer[0..10]);

        // Convert Game Boy colors to ARGB and scale
        for y in 0..SCREEN_HEIGHT {
            for x in 0..SCREEN_WIDTH {
                let color_idx = memory.ppu.frame_buffer[y * SCREEN_WIDTH + x] as usize;
                let argb = palette[color_idx % palette.len()];

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