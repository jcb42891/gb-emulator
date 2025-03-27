use std::fs;
use log::{info, error};
use std::env;

struct Memory {
    rom: Vec<u8>,
    wram: [u8; 0x2000], // 0xC000–0xDFFF
    io: [u8; 0x80],     // 0xFF00–0xFF7F
    ie: u8,             // 0xFFFF
}

impl Memory {
    fn new(rom_data: Vec<u8>) -> Self {
        Memory {
            rom: rom_data,
            wram: [0; 0x2000],
            io: [0; 0x80],
            ie: 0,
        }
    }

    fn read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => self.rom[addr as usize],
            0xC000..=0xDFFF => self.wram[(addr - 0xC000) as usize],
            0xFF00..=0xFF7F => self.io[(addr - 0xFF00) as usize],
            0xFFFF => self.ie,
            _ => {
                error!("Unhandled memory read at {:04x}", addr);
                0
            }
        }
    }

    fn write(&mut self, addr: u16, value: u8) {
        match addr {
            0xC000..=0xDFFF => self.wram[(addr - 0xC000) as usize] = value,
            0xFF00..=0xFF7F => self.io[(addr - 0xFF00) as usize] = value,
            0xFFFF => self.ie = value,
            _ => error!("Unhandled memory write at {:04x} = {:02x}", addr, value),
        }
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
            pc: 0x100,
            sp: 0,
            a: 0,
            b: 0,
            c: 0,
            d: 0,
            e: 0,
            h: 0,
            l: 0,
            f: 0,
            total_cycles: 0,
            ime: true,
        }
    }

    fn step(&mut self, memory: &mut Memory) -> u8 {
        let opcode = memory.read(self.pc);
        info!("PC: {:04x}, Opcode: {:02x}", self.pc, opcode);

        let cycles = match opcode {
            0x00 => { // NOP
                self.pc += 1;
                4
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
            _ => {
                error!("Unknown opcode: {:02x}", opcode);
                self.pc += 1;
                4
            }
        };
        self.total_cycles += cycles as u64;
        info!("Total cycles: {}", self.total_cycles);
        cycles
    }
}

fn main() {
    env_logger::init();
    let args: Vec<String> = env::args().collect();

    if args.len() != 2 {
        error!("Usage: {} <rom_path>", args[0]);
        return;
    }

    let rom_path = &args[1];
    println!("ROM PATH: {}", rom_path);
    println!("Starting gameboy emulator...");

    match fs::read(rom_path) {
        Ok(rom_data) => {
            info!("Loaded ROM with size: {} bytes", rom_data.len());

            if rom_data.len() < 0x150 {
                error!("ROM too small to have a header!");
                return;
            }

            let entry_point = &rom_data[0x100..0x104];
            info!("Entry point: {:02x} {:02x} {:02x} {:02x}",
                entry_point[0], entry_point[1], entry_point[2], entry_point[3]);

            let title_bytes = &rom_data[0x134..0x144];
            let title_str = String::from_utf8_lossy(title_bytes);
            let title = title_str.trim_end_matches(char::from(0));
            info!("Game title: {}", title);

            info!("Bytes at 0x0150: {:02x} {:02x} {:02x}",
                rom_data[0x0150], rom_data[0x0151], rom_data[0x0152]);
            info!("Bytes at 0x0159: {:02x} {:02x} {:02x}",
                rom_data[0x0159], rom_data[0x015A], rom_data[0x015B]);
            info!("Bytes at 0x015C: {:02x} {:02x} {:02x}",
                rom_data[0x015C], rom_data[0x015D], rom_data[0x015E]);
            info!("Bytes at 0x1F54: {:02x} {:02x} {:02x}",
                rom_data[0x1F54], rom_data[0x1F55], rom_data[0x1F56]);
            info!("Bytes at 0x1F57: {:02x} {:02x} {:02x}",
                rom_data[0x1F57], rom_data[0x1F58], rom_data[0x1F59]);
            info!("Bytes at 0x1F5A: {:02x} {:02x} {:02x}",
                rom_data[0x1F5A], rom_data[0x1F5B], rom_data[0x1F5C]);
            info!("Bytes at 0x1F5D: {:02x} {:02x} {:02x}",
                rom_data[0x1F5D], rom_data[0x1F5E], rom_data[0x1F5F]);
            info!("Bytes at 0x1F60: {:02x} {:02x} {:02x}",
                rom_data[0x1F60], rom_data[0x1F61], rom_data[0x1F62]);
            info!("Bytes at 0x0038: {:02x} {:02x} {:02x}",
                rom_data[0x0038], rom_data[0x0039], rom_data[0x003A]);

            let mut memory = Memory::new(rom_data);
            let mut cpu = Cpu::new();
            for _ in 0..15 { // Up to 15 to hit 0x1F5E
                cpu.step(&mut memory);
            }
        }
        Err(e) => {
            error!("Failed to load ROM: {}", e);
            println!("Error: Failed to load ROM: {}", e);
        }
    }
}