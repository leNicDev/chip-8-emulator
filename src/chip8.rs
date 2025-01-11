extern crate rand;

use std::sync::mpmc::{Receiver, Sender};
use std::thread;
use std::time::Duration;

use log::{error, info};
use rand::Rng;

const SCREEN_WIDTH: usize = 64;
const SCREEN_HEIGHT: usize = 32;
const SCREEN_SIZE: usize = SCREEN_WIDTH * SCREEN_HEIGHT;

#[derive(PartialEq, Eq)]
enum SystemState {
    Quit,
    Running,
    Paused,
}

pub struct System {
    state: SystemState,
    memory: [u8; 4096],
    v: [u8; 16],
    i: u16,
    pc: u16,
    delay_timer: u8,
    sound_timer: u8,
    stack: [u16; 12],
    sp: u16,
    keys: [bool; 16],
    gfx: [bool; SCREEN_SIZE],
    redraw_required: bool,
}

impl System {
    pub fn new() -> Self {
        return Self {
            state: SystemState::Quit,
            memory: [0; 4096],
            v: [0; 16],
            i: 0,
            pc: 0x200,
            delay_timer: 0,
            sound_timer: 0,
            stack: [0; 12],
            sp: 0,
            keys: [false; 16],
            gfx: [false; SCREEN_SIZE],
            redraw_required: true,
        };
    }

    fn reset<'a>(&'a mut self) {
        self.state = SystemState::Quit;
        self.memory = [0; 4096];
        self.v = [0; 16];
        self.i = 0;
        self.pc = 0x200;
        self.delay_timer = 0;
        self.sound_timer = 0;
        self.stack = [0; 12];
        self.sp = 0;
        self.keys = [false; 16];
        self.gfx = [false; SCREEN_SIZE];
        self.redraw_required = true;
    }

    pub fn run<'a>(&'a mut self, tx_draw: &Sender<[bool; SCREEN_SIZE]>, rx_quit: &Receiver<bool>) {
        self.state = SystemState::Running;

        // TODO: remove debug pixels
        self.gfx[0] = true;
        self.gfx[SCREEN_WIDTH - 1] = true;
        self.gfx[SCREEN_WIDTH * SCREEN_HEIGHT - SCREEN_WIDTH] = true;
        self.gfx[SCREEN_SIZE - 1] = true;

        while self.state == SystemState::Running {
            if let Ok(_) = rx_quit.try_recv() {
                self.state = SystemState::Quit;
            }

            self.cycle();

            // send redraw message
            if self.redraw_required {
                tx_draw.send(self.gfx.clone()).unwrap();
                self.redraw_required = false;
            }
        }
    }

    pub fn load_rom<'a>(&'a mut self, rom: &Vec<u8>) {
        for offset in 0..rom.len() {
            self.memory[0x200 + offset] = rom[offset];
        }
    }

    fn cycle<'a>(&'a mut self) {
        let lo = self.memory[self.pc as usize];
        let hi = self.memory[self.pc as usize + 1];
        let opcode = (lo as u16) << 8 | hi as u16;
        let pc = self.pc;
        info!("PC: {pc:#06X}\nOP: {opcode:#06X}");

        let opcode_valid = match opcode & 0xF000 {
            0x0000 => self.op_0xxx(opcode),
            0x1000 => self.op_1xxx(opcode),
            0x2000 => self.op_2xxx(opcode),
            0x3000 => self.op_3xxx(opcode),
            0x4000 => self.op_4xxx(opcode),
            0x5000 => self.op_5xxx(opcode),
            0x6000 => self.op_6xxx(opcode),
            0x7000 => self.op_7xxx(opcode),
            0x8000 => self.op_8xxx(opcode),
            0x9000 => self.op_9xxx(opcode),
            0xa000 => self.op_axxx(opcode),
            0xb000 => self.op_bxxx(opcode),
            0xc000 => self.op_cxxx(opcode),
            0xd000 => self.op_dxxx(opcode),
            0xe000 => self.op_exxx(opcode),
            0xf000 => self.op_fxxx(opcode),
            _ => false,
        };

        if !opcode_valid {
            let address = self.pc;
            error!("Invalid opcode {opcode:#06x} at address {address:#06x}");
        }

        thread::sleep(Duration::from_millis(500));
    }

    fn op_0xxx<'a>(&'a mut self, opcode: u16) -> bool {
        return match opcode {
            0x00E0 => {
                // 00E0: clear the screen
                for i in 0..self.gfx.len() {
                    self.gfx[i] = false;
                }
                self.redraw_required = true;
                self.next_instruction();
                true
            }
            0x00EE => false,
            _ => false,
        };
    }
    fn op_1xxx<'a>(&'a mut self, opcode: u16) -> bool {
        // 1NNN: jump to address NNN
        self.pc = opcode & 0x0FFF;
        true
    }
    fn op_2xxx<'a>(&'a mut self, opcode: u16) -> bool {
        // 2NNN: call subroutine at NNN
        let address = opcode & 0x0FFF;
        self.stack[self.sp as usize] = self.pc;
        self.sp += 1;
        self.pc = address;
        true
    }
    fn op_3xxx<'a>(&'a mut self, opcode: u16) -> bool {
        // 3XNN: skip the next instruction if v[x] equals NN
        let x = ((opcode & 0x0F00) >> 8) as u8;
        if self.v[x as usize] == (opcode & 0x00FF) as u8 {
            self.next_instruction();
        }
        self.next_instruction();
        true
    }
    fn op_4xxx<'a>(&'a mut self, opcode: u16) -> bool {
        // 4XNN: skip the next instruction if v[x] does not equal NN
        let x = ((opcode & 0x0F00) >> 8) as u8;
        if self.v[x as usize] != (opcode & 0x00FF) as u8 {
            self.next_instruction();
        }
        self.next_instruction();
        true
    }
    fn op_5xxx<'a>(&'a mut self, opcode: u16) -> bool {
        // 5XY0: skip the next instruction if v[x] equals v[y]
        let x = ((opcode & 0x0F00) >> 8) as u8;
        let y = ((opcode & 0x00F0) >> 4) as u8;
        if x == y {
            self.next_instruction();
        }
        self.next_instruction();
        true
    }
    fn op_6xxx<'a>(&'a mut self, opcode: u16) -> bool {
        // 6XNN: set v[x] to NN
        let x = ((opcode & 0x0F00) >> 8) as u8;
        self.v[x as usize] = (opcode & 0x00FF) as u8;
        self.next_instruction();
        true
    }
    fn op_7xxx<'a>(&'a mut self, opcode: u16) -> bool {
        // 7XNN: add NN to v[x]. does not change carry flag
        let x = ((opcode & 0x0F00) >> 8) as u8;
        self.v[x as usize] = self.v[x as usize] + ((opcode & 0x00FF) as u8 & 0xFF);
        self.next_instruction();
        true
    }
    fn op_8xxx<'a>(&'a mut self, opcode: u16) -> bool {
        let x = ((opcode & 0x0F00) >> 8) as u8 as usize;
        let y = ((opcode & 0x00F0) >> 4) as u8 as usize;

        return match opcode & 0x000F {
            0x0000 => {
                // 0x8XY0: set v[x] to v[y]
                self.v[x] = self.v[y];
                self.next_instruction();
                true
            }
            0x0001 => {
                // 0x8XY1: set v[x] to (v[x] | v[y])
                self.v[x] = self.v[x] | self.v[y];
                self.next_instruction();
                true
            }
            0x0002 => {
                // 0x8XY2: set v[x] to (v[x] & v[y])
                self.v[x] = self.v[x] & self.v[y];
                self.next_instruction();
                true
            }
            0x0003 => {
                // 0x8XY3: set v[x] to (v[x] ^ v[y])
                self.v[x] = self.v[x] ^ self.v[y];
                self.next_instruction();
                true
            }
            0x0004 => {
                // 0x8XY4: add v[y] to v[x]. set v[0xF] to 1 when overflow happened and to 0 when not
                if self.v[x] > 255 - self.v[y] {
                    self.v[0xF] = 1;
                } else {
                    self.v[0xF] = 0;
                }
                self.v[x] += self.v[y];
                self.next_instruction();
                true
            }
            0x0005 => {
                // 0x8XY5: subtract v[y] from v[x]. set v[0xF] to 0 when underflow happened and to 1 when not
                if self.v[x] < self.v[y] {
                    self.v[0xF] = 0;
                } else {
                    self.v[0xF] = 1;
                }
                self.v[x] -= self.v[y];
                self.next_instruction();
                true
            }
            0x0006 => {
                // 0x8XY6: shift v[x] to the right by 1 then store least significant bit of v[x]
                // prior to the shift into v[0xF]
                self.v[0xF] = self.v[x] & 0x01;
                self.v[x] = self.v[x] >> 1;
                self.next_instruction();
                true
            }
            0x0007 => {
                // 0x8XY7: set v[x] to (v[y] - v[x]). set v[0xF] to 1 when underflow happened and to 0 when not
                if self.v[y] < self.v[x] {
                    self.v[0xF] = 0;
                } else {
                    self.v[0xF] = 1;
                }
                self.v[x] = self.v[y] - self.v[x];
                self.next_instruction();
                true
            }
            0x000E => {
                // 0x8XYE: shift v[x] to the left by 1 then set v[0xF] to 1 if the
                // most significant bit of v[x] prior to the shift was set
                // or else set v[0xF] to 0
                if self.v[x] >> 7 == 1 {
                    self.v[0xF] = 1;
                } else {
                    self.v[0xF] = 0;
                }
                self.v[x] <<= 1;
                self.next_instruction();
                true
            }
            _ => false,
        };
    }
    fn op_9xxx<'a>(&'a mut self, opcode: u16) -> bool {
        if opcode & 0x000F != 0 {
            return false;
        }

        // 9XY0: skip the next instruction if v[x] does not equal v[y]
        let x = ((opcode & 0x0F00) >> 8) as u8 as usize;
        let y = ((opcode & 0x00F0) >> 4) as u8 as usize;

        if self.v[x] == self.v[y] {
            self.next_instruction();
        } else {
            self.pc += 4;
        }

        true
    }
    fn op_axxx<'a>(&'a mut self, opcode: u16) -> bool {
        // ANNN set i to the address NNN
        self.i = opcode & 0x0FFF;
        self.next_instruction();
        true
    }
    fn op_bxxx<'a>(&'a mut self, opcode: u16) -> bool {
        // BNNN: jump to the address NNN plus v[0x0]
        self.i = (opcode & 0x0FFF) + self.v[0] as u16;
        self.next_instruction();
        true
    }
    fn op_cxxx<'a>(&'a mut self, opcode: u16) -> bool {
        // CXNN: set v[x] to the result of (random_u8() & nn)
        let x = ((opcode & 0x0F00) >> 8) as u8 as usize;
        let nn = (opcode & 0x00FF) as u8;
        self.v[x] = random_u8() & nn;
        self.next_instruction();
        true
    }
    fn op_dxxx<'a>(&'a mut self, opcode: u16) -> bool {
        // DXYN: draw sprite at coordinate (v[x], v[y]) that is 8xN pixels in size
        let start_x = ((opcode & 0x0F00) >> 8) as u8;
        let start_y = ((opcode & 0x00F0) >> 4) as u8;
        let height = (opcode & 0x000F) as u8;

        self.v[0xF] = 0;

        for y in 0..height {
            let line = self.memory[self.i as usize + y as usize];
            for x in 0..8 {
                let pixel = line & (0x80 >> x);
                if pixel != 0 {
                    let total_x = start_x + x;
                    let total_y = start_y + y;
                    let index = (total_y as usize * SCREEN_WIDTH) + total_x as usize;

                    if self.gfx[index] {
                        self.v[0xF] = 1;
                    }
                    self.gfx[index] = !self.gfx[index]; // is this correct?
                }
            }
        }

        self.redraw_required = true;
        self.next_instruction();
        true
    }
    fn op_exxx<'a>(&'a mut self, opcode: u16) -> bool {
        let x = ((opcode & 0x0F00) >> 8) as u8 as usize;
        let key = self.keys[(self.v[x] & 0x0F) as usize];

        return match opcode & 0x00FF {
            0x009E => {
                // EX9E: skip the next instruction if key stored in v[x]
                // (only lowest nibble) is pressed
                if key {
                    self.next_instruction();
                }
                self.next_instruction();
                true
            }
            0x00A1 => {
                // EXA1: skip the next instruction if key stored in v[x]
                // (only lowest nibble) is not pressed
                if !key {
                    self.next_instruction();
                }
                self.next_instruction();
                true
            }
            _ => false,
        };
    }
    fn op_fxxx<'a>(&'a mut self, opcode: u16) -> bool {
        let x = ((opcode & 0x0F00) >> 8) as u8 as usize;

        return match opcode & 0x00FF {
            0x0007 => {
                // FX0A: set v[x] to the value of the delay timer
                self.v[x] = self.delay_timer;
                self.next_instruction();
                true
            }
            0x000A => {
                // TODO: FX15: wait for key press and store it in v[x].
                // this is a blocking operation. halt all instructions
                // until next key event. timers should continue processing
                false
            }
            0x0015 => {
                self.delay_timer = self.v[x];
                self.next_instruction();
                true
            }
            0x0018 => {
                // FX18: set the sound timer to the value of v[x]
                self.sound_timer = self.v[x];
                self.next_instruction();
                true
            }
            0x001E => {
                // FX1E: add v[x] to i (v[0xF] is not affected)
                self.i += self.v[x] as u16;
                self.next_instruction();
                true
            }
            0x0029 => {
                // FX29: set i to the location of the sprite for the character
                // v[x] (only consider lowest nibble).
                // characters 0x0-0xF are represented by a 4x5 font
                false
            }
            0x0033 => {
                // FX33: store the binary-coded decimal representation of v[x]
                // with the hundreds digit in memory at location i.
                // the tens digit at location i+1 and the ones digit at i+2
                let hundreds = self.v[x] / 100;
                let tens = (self.v[x] - hundreds) / 10;
                let ones = self.v[x] % 10;
                self.memory[self.i as usize] = hundreds;
                self.memory[self.i as usize + 1] = tens;
                self.memory[self.i as usize + 2] = ones;
                self.next_instruction();
                true
            }
            0x0055 => {
                // FX55: store registers v[0x0] to v[x] (including v[x]) in memory
                // starting at address i
                for offset in 0..self.v[x] as usize {
                    self.memory[self.i as usize + offset] = self.v[offset];
                }
                self.next_instruction();
                true
            }
            0x0065 => {
                // FX65: fill registers v[0x0] to v[x] (including v[x]) with values
                // from memory starting at address i
                for offset in 0..self.v[x] as usize {
                    self.v[offset] = self.memory[self.i as usize + offset];
                }
                self.next_instruction();
                true
            }
            _ => false,
        };
    }

    fn next_instruction<'a>(&'a mut self) {
        self.pc += 2;
    }
}

fn random_u8() -> u8 {
    let num: u8 = rand::thread_rng().r#gen();
    return num;
}
