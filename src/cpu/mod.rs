mod op;

use crate::bus::Bus;
use crate::error::NesError;
use crate::cart::Rom;
use crate::cart::Cartridge;
use op::{AddressingMode, DecodedOp, Instruction, Mnemonic, Operand, decode};

/// Status flag bits in P (NV-BDIZC).
const FLAG_C: u8 = 1 << 0;
const FLAG_Z: u8 = 1 << 1;
const FLAG_I: u8 = 1 << 2;
const FLAG_D: u8 = 1 << 3;
const FLAG_B: u8 = 1 << 4;
const FLAG_U: u8 = 1 << 5; // unused; always set when pushed
const FLAG_V: u8 = 1 << 6;
const FLAG_N: u8 = 1 << 7;

/// Resolved operand / effective address from addressing mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effective {
    Implied,
    Accumulator,
    Immediate(u8),
    /// Memory operand; `page_crossed` is set for indexed modes that crossed a page.
    Address {
        addr: u16,
        page_crossed: bool,
    },
    Relative(i8),
}

pub struct Cpu {
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub sp: u8,
    pub pc: u16,
    pub p: u8,
    /// Total cycles elapsed since power-on/reset.
    pub cycles: u64,
    /// Memory accesses go through the bus (RAM, PRG, later PPU/APU).
    pub bus: Bus,
}

impl Cpu {
    /// Build a CPU for unit tests: small buffers go into WRAM; larger ones become NROM.
    pub fn new(prg: Vec<u8>) -> Self {
        let mut bus = Bus::new();
        let pc = bus.load_prg(prg);
        Self::with_bus(bus, pc)
    }

    /// Step 3 integration: parse path is `Loader` → `Rom` → `Cartridge` → `Bus`.
    pub fn from_rom(rom: Rom) -> Result<Self, NesError> {
        let cart = Cartridge::from_rom(rom)?;
        Ok(Self::from_cartridge(cart))
    }

    pub fn from_cartridge(cart: Cartridge) -> Self {
        let mut bus = Bus::new();
        let pc = bus.insert_cartridge(cart);
        Self::with_bus(bus, pc)
    }

    fn with_bus(bus: Bus, pc: u16) -> Self {
        Self {
            a: 0,
            x: 0,
            y: 0,
            sp: 0xFD,
            pc,
            p: FLAG_U | FLAG_I,
            cycles: 0,
            bus,
        }
    }

    /// One machine step, keeping CPU and PPU/APU in sync:
    ///
    /// 1. Service pending NMI (PPU VBlank) or IRQ (APU), if any  
    /// 2. Fetch → decode → execute via Bus  
    /// 3. Tick PPU (×3) + APU by the instruction's CPU cycles  
    /// 4. Drain OAM-DMA cycles if `$4014` was written  
    ///
    /// Prefer [`crate::nes::Nes::run_frame`] for frame-oriented emulation.
    pub fn step(&mut self) -> Result<(), NesError> {
        if self.bus.ppu.take_nmi() {
            self.handle_nmi()?;
            return Ok(());
        }
        let cart_irq = self
            .bus
            .cartridge()
            .map(|c| c.irq_pending())
            .unwrap_or(false);
        if !self.flag(FLAG_I) && (self.bus.apu.irq_pending() || cart_irq) {
            self.handle_irq()?;
            return Ok(());
        }

        let _opcode = self.fetch()?;
        let (decoded, effective) = self.decode_instruction()?;
        let cycles = self.execute(&decoded.instruction, decoded.next as u16, effective)? as u32;
        self.bus.tick(cycles);
        self.add_cycles(cycles as u8);

        let dma = self.bus.take_dma_cycles();
        if dma > 0 {
            self.bus.tick(dma);
            self.cycles = self.cycles.wrapping_add(dma as u64);
        }
        Ok(())
    }

    /// Run until the PPU finishes the current frame (frame counter increments).
    pub fn run_frame(&mut self) -> Result<(), NesError> {
        let start = self.bus.ppu.frame;
        let limit = self.bus.clock.cpu_cycles_per_frame() as u64 * 8;
        let cycles_at_start = self.cycles;

        while self.bus.ppu.frame == start {
            self.step()?;
            if self.cycles.wrapping_sub(cycles_at_start) > limit {
                return Err(NesError::GenericError);
            }
        }
        Ok(())
    }

    /// Continuous run — advances frame by frame (still infinite; use for long sessions).
    pub fn run(&mut self) -> Result<(), NesError> {
        loop {
            self.run_frame()?;
        }
    }

    fn handle_nmi(&mut self) -> Result<(), NesError> {
        self.push_u16(self.pc);
        // Interrupt push: B flag clear in the copy on the stack.
        self.push((self.p | FLAG_U) & !FLAG_B);
        self.set_flag(FLAG_I, true);
        self.pc = self.read_u16(0xFFFA);
        self.bus.tick(7);
        self.add_cycles(7);
        Ok(())
    }

    fn handle_irq(&mut self) -> Result<(), NesError> {
        self.push_u16(self.pc);
        self.push((self.p | FLAG_U) & !FLAG_B);
        self.set_flag(FLAG_I, true);
        self.pc = self.read_u16(0xFFFE);
        self.bus.tick(7);
        self.add_cycles(7);
        Ok(())
    }

    /// Fetch the opcode byte at PC (does not advance PC).
    pub fn fetch(&mut self) -> Result<u8, NesError> {
        Ok(self.read(self.pc))
    }

    /// Decode instruction at PC and resolve operand / effective address.
    pub fn decode_instruction(&mut self) -> Result<(DecodedOp, Effective), NesError> {
        // Official opcodes are at most 3 bytes; fetch a window through the bus.
        let window = [
            self.bus.read(self.pc),
            self.bus.read(self.pc.wrapping_add(1)),
            self.bus.read(self.pc.wrapping_add(2)),
        ];
        let mut decoded = decode(0, &window)?;
        // `decode` returns next relative to the window; convert to absolute PC.
        decoded.next = self.pc as usize + decoded.next;
        let effective = self.resolve_effective(&decoded.instruction)?;
        Ok((decoded, effective))
    }

    fn resolve_effective(&mut self, inst: &Instruction) -> Result<Effective, NesError> {
        Ok(match inst.mode {
            // Không có operand (CLC, TAX, RTS, …).
            AddressingMode::Implied => Effective::Implied,

            // Thao tác trên A (ASL A, ROL A, …).
            AddressingMode::Accumulator => Effective::Accumulator,

            // Operand chính là giá trị tức thì: LDA #$42 → Immediate(0x42).
            AddressingMode::Immediate => {
                let Operand::Byte(v) = inst.operand else {
                    return Err(NesError::GenericError);
                };
                Effective::Immediate(v)
            }

            // Offset có dấu cho branch; PC đích = next_pc + offset (tính trong execute).
            AddressingMode::Relative => {
                let Operand::Byte(v) = inst.operand else {
                    return Err(NesError::GenericError);
                };
                Effective::Relative(v as i8)
            }

            // Địa chỉ 8-bit trên zero page: $00LL.
            AddressingMode::ZeroPage => {
                let Operand::Byte(v) = inst.operand else {
                    return Err(NesError::GenericError);
                };
                Effective::Address {
                    addr: v as u16,
                    page_crossed: false,
                }
            }

            // Zero page + X, wrap trong trang 0 (chỉ 8-bit, không có page-cross penalty).
            AddressingMode::ZeroPageX => {
                let Operand::Byte(v) = inst.operand else {
                    return Err(NesError::GenericError);
                };
                Effective::Address {
                    addr: v.wrapping_add(self.x) as u16,
                    page_crossed: false,
                }
            }

            // Zero page + Y (LDX/STX), wrap 8-bit như ZeroPageX.
            AddressingMode::ZeroPageY => {
                let Operand::Byte(v) = inst.operand else {
                    return Err(NesError::GenericError);
                };
                Effective::Address {
                    addr: v.wrapping_add(self.y) as u16,
                    page_crossed: false,
                }
            }

            // Địa chỉ 16-bit tuyệt đối: LDA $1234.
            AddressingMode::Absolute => {
                let Operand::Word(addr) = inst.operand else {
                    return Err(NesError::GenericError);
                };
                Effective::Address {
                    addr,
                    page_crossed: false,
                }
            }

            // Absolute + X; đánh dấu page_crossed nếu vượt biên trang 256 byte.
            AddressingMode::AbsoluteX => {
                let Operand::Word(base) = inst.operand else {
                    return Err(NesError::GenericError);
                };
                let addr = base.wrapping_add(self.x as u16);
                Effective::Address {
                    addr,
                    page_crossed: page_crossed(base, addr),
                }
            }

            // Absolute + Y — giống AbsoluteX nhưng cộng Y.
            AddressingMode::AbsoluteY => {
                let Operand::Word(base) = inst.operand else {
                    return Err(NesError::GenericError);
                };
                let addr = base.wrapping_add(self.y as u16);
                Effective::Address {
                    addr,
                    page_crossed: page_crossed(base, addr),
                }
            }

            // JMP ($hhll): đọc word tại ptr làm đích.
            // Bug 6502: nếu ptr = $xxFF thì byte high lấy ở $xx00, không phải $xy00.
            AddressingMode::Indirect => {
                let Operand::Word(ptr) = inst.operand else {
                    return Err(NesError::GenericError);
                };
                let lo = self.read(ptr) as u16;
                let hi_addr = (ptr & 0xFF00) | (ptr.wrapping_add(1) & 0x00FF);
                let hi = self.read(hi_addr) as u16;
                Effective::Address {
                    addr: lo | (hi << 8),
                    page_crossed: false,
                }
            }

            // (zp,X): cộng X vào pointer zero-page (wrap 8-bit), rồi đọc word đó làm địa chỉ.
            AddressingMode::IndirectX => {
                let Operand::Byte(v) = inst.operand else {
                    return Err(NesError::GenericError);
                };
                let ptr = v.wrapping_add(self.x);
                let lo = self.read(ptr as u16) as u16;
                let hi = self.read(ptr.wrapping_add(1) as u16) as u16;
                Effective::Address {
                    addr: lo | (hi << 8),
                    page_crossed: false,
                }
            }

            // (zp),Y: đọc word tại zp trước, rồi cộng Y; có thể page-cross.
            AddressingMode::IndirectY => {
                let Operand::Byte(v) = inst.operand else {
                    return Err(NesError::GenericError);
                };
                let lo = self.read(v as u16) as u16;
                let hi = self.read(v.wrapping_add(1) as u16) as u16;
                let base = lo | (hi << 8);
                let addr = base.wrapping_add(self.y as u16);
                Effective::Address {
                    addr,
                    page_crossed: page_crossed(base, addr),
                }
            }
        })
    }

    // -------------------------------------------------------------------------
    // (3) Execute side-effects
    // -------------------------------------------------------------------------

    /// Apply instruction side-effects; returns base(+penalty) cycle count.
    pub fn execute(
        &mut self,
        inst: &Instruction,
        next_pc: u16,
        eff: Effective,
    ) -> Result<u8, NesError> {
        // Default: advance PC past this instruction. Control-flow ops override.
        self.pc = next_pc;

        let page_crossed = matches!(
            eff,
            Effective::Address {
                page_crossed: true,
                ..
            }
        );
        let mut extra: u8 = 0;

        match inst.mnemonic {
            Mnemonic::Adc => {
                let v = self.operand_value(eff)?;
                self.adc(v);
                extra = read_page_penalty(inst.mode, page_crossed);
            }
            Mnemonic::And => {
                let v = self.operand_value(eff)?;
                self.a &= v;
                self.set_zn(self.a);
                extra = read_page_penalty(inst.mode, page_crossed);
            }
            Mnemonic::Asl => match eff {
                Effective::Accumulator => {
                    self.set_flag(FLAG_C, self.a & 0x80 != 0);
                    self.a <<= 1;
                    self.set_zn(self.a);
                }
                Effective::Address { addr, .. } => {
                    let mut v = self.read(addr);
                    self.set_flag(FLAG_C, v & 0x80 != 0);
                    v <<= 1;
                    self.write(addr, v);
                    self.set_zn(v);
                }
                _ => return Err(NesError::GenericError),
            },
            Mnemonic::Bcc => {
                extra = self.branch(eff, !self.flag(FLAG_C))?;
            }
            Mnemonic::Bcs => {
                extra = self.branch(eff, self.flag(FLAG_C))?;
            }
            Mnemonic::Beq => {
                extra = self.branch(eff, self.flag(FLAG_Z))?;
            }
            Mnemonic::Bit => {
                let Effective::Address { addr, .. } = eff else {
                    return Err(NesError::GenericError);
                };
                let v = self.read(addr);
                self.set_flag(FLAG_Z, self.a & v == 0);
                self.set_flag(FLAG_V, v & 0x40 != 0);
                self.set_flag(FLAG_N, v & 0x80 != 0);
            }
            Mnemonic::Bmi => {
                extra = self.branch(eff, self.flag(FLAG_N))?;
            }
            Mnemonic::Bne => {
                extra = self.branch(eff, !self.flag(FLAG_Z))?;
            }
            Mnemonic::Bpl => {
                extra = self.branch(eff, !self.flag(FLAG_N))?;
            }
            Mnemonic::Brk => {
                // Return address skips the padding byte after BRK.
                let return_pc = next_pc.wrapping_add(1);
                self.push_u16(return_pc);
                self.push(self.p | FLAG_B | FLAG_U);
                self.set_flag(FLAG_I, true);
                self.pc = self.read_u16(0xFFFE);
            }
            Mnemonic::Bvc => {
                extra = self.branch(eff, !self.flag(FLAG_V))?;
            }
            Mnemonic::Bvs => {
                extra = self.branch(eff, self.flag(FLAG_V))?;
            }
            Mnemonic::Clc => self.set_flag(FLAG_C, false),
            Mnemonic::Cld => self.set_flag(FLAG_D, false),
            Mnemonic::Cli => self.set_flag(FLAG_I, false),
            Mnemonic::Clv => self.set_flag(FLAG_V, false),
            Mnemonic::Cmp => {
                let v = self.operand_value(eff)?;
                self.compare(self.a, v);
                extra = read_page_penalty(inst.mode, page_crossed);
            }
            Mnemonic::Cpx => {
                let v = self.operand_value(eff)?;
                self.compare(self.x, v);
            }
            Mnemonic::Cpy => {
                let v = self.operand_value(eff)?;
                self.compare(self.y, v);
            }
            Mnemonic::Dec => {
                let Effective::Address { addr, .. } = eff else {
                    return Err(NesError::GenericError);
                };
                let v = self.read(addr).wrapping_sub(1);
                self.write(addr, v);
                self.set_zn(v);
            }
            Mnemonic::Dex => {
                self.x = self.x.wrapping_sub(1);
                self.set_zn(self.x);
            }
            Mnemonic::Dey => {
                self.y = self.y.wrapping_sub(1);
                self.set_zn(self.y);
            }
            Mnemonic::Eor => {
                let v = self.operand_value(eff)?;
                self.a ^= v;
                self.set_zn(self.a);
                extra = read_page_penalty(inst.mode, page_crossed);
            }
            Mnemonic::Inc => {
                let Effective::Address { addr, .. } = eff else {
                    return Err(NesError::GenericError);
                };
                let v = self.read(addr).wrapping_add(1);
                self.write(addr, v);
                self.set_zn(v);
            }
            Mnemonic::Inx => {
                self.x = self.x.wrapping_add(1);
                self.set_zn(self.x);
            }
            Mnemonic::Iny => {
                self.y = self.y.wrapping_add(1);
                self.set_zn(self.y);
            }
            Mnemonic::Jmp => {
                let Effective::Address { addr, .. } = eff else {
                    return Err(NesError::GenericError);
                };
                self.pc = addr;
            }
            Mnemonic::Jsr => {
                let Effective::Address { addr, .. } = eff else {
                    return Err(NesError::GenericError);
                };
                // Push address of last byte of JSR (next_pc - 1).
                self.push_u16(next_pc.wrapping_sub(1));
                self.pc = addr;
            }
            Mnemonic::Lda => {
                let v = self.operand_value(eff)?;
                self.a = v;
                self.set_zn(self.a);
                extra = read_page_penalty(inst.mode, page_crossed);
            }
            Mnemonic::Ldx => {
                let v = self.operand_value(eff)?;
                self.x = v;
                self.set_zn(self.x);
                extra = read_page_penalty(inst.mode, page_crossed);
            }
            Mnemonic::Ldy => {
                let v = self.operand_value(eff)?;
                self.y = v;
                self.set_zn(self.y);
                extra = read_page_penalty(inst.mode, page_crossed);
            }
            Mnemonic::Lsr => match eff {
                Effective::Accumulator => {
                    self.set_flag(FLAG_C, self.a & 0x01 != 0);
                    self.a >>= 1;
                    self.set_zn(self.a);
                }
                Effective::Address { addr, .. } => {
                    let mut v = self.read(addr);
                    self.set_flag(FLAG_C, v & 0x01 != 0);
                    v >>= 1;
                    self.write(addr, v);
                    self.set_zn(v);
                }
                _ => return Err(NesError::GenericError),
            },
            Mnemonic::Nop => {}
            Mnemonic::Ora => {
                let v = self.operand_value(eff)?;
                self.a |= v;
                self.set_zn(self.a);
                extra = read_page_penalty(inst.mode, page_crossed);
            }
            Mnemonic::Pha => self.push(self.a),
            Mnemonic::Php => self.push(self.p | FLAG_B | FLAG_U),
            Mnemonic::Pla => {
                self.a = self.pull();
                self.set_zn(self.a);
            }
            Mnemonic::Plp => {
                // B is not a real flag; U stays set in our P representation.
                self.p = (self.pull() & !FLAG_B) | FLAG_U;
            }
            Mnemonic::Rol => match eff {
                Effective::Accumulator => {
                    let carry_in = self.flag(FLAG_C) as u8;
                    self.set_flag(FLAG_C, self.a & 0x80 != 0);
                    self.a = (self.a << 1) | carry_in;
                    self.set_zn(self.a);
                }
                Effective::Address { addr, .. } => {
                    let carry_in = self.flag(FLAG_C) as u8;
                    let mut v = self.read(addr);
                    self.set_flag(FLAG_C, v & 0x80 != 0);
                    v = (v << 1) | carry_in;
                    self.write(addr, v);
                    self.set_zn(v);
                }
                _ => return Err(NesError::GenericError),
            },
            Mnemonic::Ror => match eff {
                Effective::Accumulator => {
                    let carry_in = if self.flag(FLAG_C) { 0x80 } else { 0 };
                    self.set_flag(FLAG_C, self.a & 0x01 != 0);
                    self.a = (self.a >> 1) | carry_in;
                    self.set_zn(self.a);
                }
                Effective::Address { addr, .. } => {
                    let carry_in = if self.flag(FLAG_C) { 0x80 } else { 0 };
                    let mut v = self.read(addr);
                    self.set_flag(FLAG_C, v & 0x01 != 0);
                    v = (v >> 1) | carry_in;
                    self.write(addr, v);
                    self.set_zn(v);
                }
                _ => return Err(NesError::GenericError),
            },
            Mnemonic::Rti => {
                self.p = (self.pull() & !FLAG_B) | FLAG_U;
                self.pc = self.pull_u16();
            }
            Mnemonic::Rts => {
                self.pc = self.pull_u16().wrapping_add(1);
            }
            Mnemonic::Sbc => {
                let v = self.operand_value(eff)?;
                // SBC = ADC with inverted operand (binary mode; NES has no BCD).
                self.adc(!v);
                extra = read_page_penalty(inst.mode, page_crossed);
            }
            Mnemonic::Sec => self.set_flag(FLAG_C, true),
            Mnemonic::Sed => self.set_flag(FLAG_D, true),
            Mnemonic::Sei => self.set_flag(FLAG_I, true),
            Mnemonic::Sta => {
                let Effective::Address { addr, .. } = eff else {
                    return Err(NesError::GenericError);
                };
                self.write(addr, self.a);
            }
            Mnemonic::Stx => {
                let Effective::Address { addr, .. } = eff else {
                    return Err(NesError::GenericError);
                };
                self.write(addr, self.x);
            }
            Mnemonic::Sty => {
                let Effective::Address { addr, .. } = eff else {
                    return Err(NesError::GenericError);
                };
                self.write(addr, self.y);
            }
            Mnemonic::Tax => {
                self.x = self.a;
                self.set_zn(self.x);
            }
            Mnemonic::Tay => {
                self.y = self.a;
                self.set_zn(self.y);
            }
            Mnemonic::Tsx => {
                self.x = self.sp;
                self.set_zn(self.x);
            }
            Mnemonic::Txa => {
                self.a = self.x;
                self.set_zn(self.a);
            }
            Mnemonic::Txs => {
                self.sp = self.x;
            }
            Mnemonic::Tya => {
                self.a = self.y;
                self.set_zn(self.a);
            }
        }

        Ok(base_cycles(inst.opcode).wrapping_add(extra))
    }

    // -------------------------------------------------------------------------
    // (4) Cycles
    // -------------------------------------------------------------------------

    pub fn add_cycles(&mut self, n: u8) {
        self.cycles = self.cycles.wrapping_add(n as u64);
    }

    // -------------------------------------------------------------------------
    // Memory / stack / flags helpers
    // -------------------------------------------------------------------------

    fn read(&mut self, addr: u16) -> u8 {
        self.bus.read(addr)
    }

    fn write(&mut self, addr: u16, value: u8) {
        self.bus.write(addr, value);
    }

    fn read_u16(&mut self, addr: u16) -> u16 {
        self.bus.read_u16(addr)
    }

    fn push(&mut self, value: u8) {
        self.write(0x0100 | self.sp as u16, value);
        self.sp = self.sp.wrapping_sub(1);
    }

    fn pull(&mut self) -> u8 {
        self.sp = self.sp.wrapping_add(1);
        self.read(0x0100 | self.sp as u16)
    }

    fn push_u16(&mut self, value: u16) {
        self.push((value >> 8) as u8);
        self.push(value as u8);
    }

    fn pull_u16(&mut self) -> u16 {
        let lo = self.pull() as u16;
        let hi = self.pull() as u16;
        lo | (hi << 8)
    }

    fn flag(&self, mask: u8) -> bool {
        self.p & mask != 0
    }

    fn set_flag(&mut self, mask: u8, on: bool) {
        if on {
            self.p |= mask;
        } else {
            self.p &= !mask;
        }
    }

    fn set_zn(&mut self, value: u8) {
        self.set_flag(FLAG_Z, value == 0);
        self.set_flag(FLAG_N, value & 0x80 != 0);
    }

    fn operand_value(&mut self, eff: Effective) -> Result<u8, NesError> {
        match eff {
            Effective::Immediate(v) => Ok(v),
            Effective::Address { addr, .. } => Ok(self.read(addr)),
            _ => Err(NesError::GenericError),
        }
    }

    fn compare(&mut self, reg: u8, value: u8) {
        let result = reg.wrapping_sub(value);
        self.set_flag(FLAG_C, reg >= value);
        self.set_zn(result);
    }

    fn adc(&mut self, value: u8) {
        let a = self.a as u16;
        let c = self.flag(FLAG_C) as u16;
        let sum = a + value as u16 + c;
        let result = sum as u8;

        // Overflow: sign of inputs same, sign of result different.
        let overflow = !(self.a ^ value) & (self.a ^ result) & 0x80;
        self.set_flag(FLAG_C, sum > 0xFF);
        self.set_flag(FLAG_V, overflow != 0);
        self.a = result;
        self.set_zn(self.a);
    }

    /// Branch helper; returns extra cycles (0 / 1 / 2).
    fn branch(&mut self, eff: Effective, taken: bool) -> Result<u8, NesError> {
        let Effective::Relative(offset) = eff else {
            return Err(NesError::GenericError);
        };
        if !taken {
            return Ok(0);
        }
        let old_pc = self.pc;
        self.pc = (self.pc as i32 + offset as i32) as u16;
        let mut extra = 1;
        if page_crossed(old_pc, self.pc) {
            extra += 1;
        }
        Ok(extra)
    }
}

fn page_crossed(a: u16, b: u16) -> bool {
    (a & 0xFF00) != (b & 0xFF00)
}

fn read_page_penalty(mode: AddressingMode, page_crossed: bool) -> u8 {
    match mode {
        AddressingMode::AbsoluteX | AddressingMode::AbsoluteY | AddressingMode::IndirectY
            if page_crossed =>
        {
            1
        }
        _ => 0,
    }
}

/// Base cycle counts for official opcodes (before page-cross / branch penalties).
fn base_cycles(opcode: u8) -> u8 {
    match opcode {
        // ADC
        0x69 => 2,
        0x65 => 3,
        0x75 => 4,
        0x6D => 4,
        0x7D => 4,
        0x79 => 4,
        0x61 => 6,
        0x71 => 5,
        // AND
        0x29 => 2,
        0x25 => 3,
        0x35 => 4,
        0x2D => 4,
        0x3D => 4,
        0x39 => 4,
        0x21 => 6,
        0x31 => 5,
        // ASL
        0x0A => 2,
        0x06 => 5,
        0x16 => 6,
        0x0E => 6,
        0x1E => 7,
        // Branches
        0x90 | 0xB0 | 0xF0 | 0x30 | 0xD0 | 0x10 | 0x50 | 0x70 => 2,
        // BIT
        0x24 => 3,
        0x2C => 4,
        // BRK
        0x00 => 7,
        // Flag ops / transfers / implied
        0x18 | 0xD8 | 0x58 | 0xB8 | 0xCA | 0x88 | 0xE8 | 0xC8 | 0xEA | 0x38 | 0xF8 | 0x78
        | 0xAA | 0xA8 | 0xBA | 0x8A | 0x9A | 0x98 => 2,
        // Stack
        0x48 | 0x08 => 3,
        0x68 | 0x28 => 4,
        0x40 | 0x60 => 6,
        // CMP
        0xC9 => 2,
        0xC5 => 3,
        0xD5 => 4,
        0xCD => 4,
        0xDD => 4,
        0xD9 => 4,
        0xC1 => 6,
        0xD1 => 5,
        // CPX / CPY
        0xE0 | 0xC0 => 2,
        0xE4 | 0xC4 => 3,
        0xEC | 0xCC => 4,
        // DEC / INC
        0xC6 | 0xE6 => 5,
        0xD6 | 0xF6 => 6,
        0xCE | 0xEE => 6,
        0xDE | 0xFE => 7,
        // EOR
        0x49 => 2,
        0x45 => 3,
        0x55 => 4,
        0x4D => 4,
        0x5D => 4,
        0x59 => 4,
        0x41 => 6,
        0x51 => 5,
        // JMP / JSR
        0x4C => 3,
        0x6C => 5,
        0x20 => 6,
        // LDA
        0xA9 => 2,
        0xA5 => 3,
        0xB5 => 4,
        0xAD => 4,
        0xBD => 4,
        0xB9 => 4,
        0xA1 => 6,
        0xB1 => 5,
        // LDX
        0xA2 => 2,
        0xA6 => 3,
        0xB6 => 4,
        0xAE => 4,
        0xBE => 4,
        // LDY
        0xA0 => 2,
        0xA4 => 3,
        0xB4 => 4,
        0xAC => 4,
        0xBC => 4,
        // LSR
        0x4A => 2,
        0x46 => 5,
        0x56 => 6,
        0x4E => 6,
        0x5E => 7,
        // ORA
        0x09 => 2,
        0x05 => 3,
        0x15 => 4,
        0x0D => 4,
        0x1D => 4,
        0x19 => 4,
        0x01 => 6,
        0x11 => 5,
        // ROL
        0x2A => 2,
        0x26 => 5,
        0x36 => 6,
        0x2E => 6,
        0x3E => 7,
        // ROR
        0x6A => 2,
        0x66 => 5,
        0x76 => 6,
        0x6E => 6,
        0x7E => 7,
        // SBC
        0xE9 => 2,
        0xE5 => 3,
        0xF5 => 4,
        0xED => 4,
        0xFD => 4,
        0xF9 => 4,
        0xE1 => 6,
        0xF1 => 5,
        // STA
        0x85 => 3,
        0x95 => 4,
        0x8D => 4,
        0x9D => 5,
        0x99 => 5,
        0x81 => 6,
        0x91 => 6,
        // STX / STY
        0x86 | 0x84 => 3,
        0x96 | 0x94 => 4,
        0x8E | 0x8C => 4,

        _ => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_lda_immediate() {
        let mut cpu = Cpu::new(vec![0xA9, 0x42, 0x00]); // LDA #$42; BRK
        cpu.step().unwrap();
        assert_eq!(cpu.a, 0x42);
        assert_eq!(cpu.pc, 2);
        assert_eq!(cpu.cycles, 2);
        assert!(!cpu.flag(FLAG_Z));
        assert!(!cpu.flag(FLAG_N));
    }

    #[test]
    fn adc_and_flags() {
        let mut cpu = Cpu::new(vec![
            0xA9, 0xF0, // LDA #$F0
            0x69, 0x20, // ADC #$20 → $10, C=1
        ]);
        cpu.step().unwrap();
        cpu.step().unwrap();
        assert_eq!(cpu.a, 0x10);
        assert!(cpu.flag(FLAG_C));
    }

    #[test]
    fn jsr_rts_roundtrip() {
        // $0000: JSR $0005; NOP; (pad); LDA #$99; RTS
        let mut cpu = Cpu::new(vec![
            0x20, 0x05, 0x00, // JSR $0005
            0xEA, // NOP (return here)
            0x00, // pad
            0xA9, 0x99, // LDA #$99
            0x60, // RTS
        ]);
        cpu.step().unwrap(); // JSR
        assert_eq!(cpu.pc, 0x0005);
        cpu.step().unwrap(); // LDA
        assert_eq!(cpu.a, 0x99);
        cpu.step().unwrap(); // RTS
        assert_eq!(cpu.pc, 0x0003);
    }

    #[test]
    fn branch_beq_taken() {
        let mut cpu = Cpu::new(vec![
            0xA9, 0x00, // LDA #$00 (Z=1)
            0xF0, 0x02, // BEQ +2
            0xA9, 0x01, // LDA #$01 (skipped)
            0xA9, 0x02, // LDA #$02
        ]);
        cpu.step().unwrap();
        cpu.step().unwrap(); // BEQ
        assert_eq!(cpu.pc, 6);
        assert!(cpu.cycles >= 2 + 2 + 1); // LDA + BEQ taken
    }

    #[test]
    fn sta_zero_page() {
        let mut cpu = Cpu::new(vec![
            0xA9, 0x42, // LDA #$42
            0x85, 0x10, // STA $10
        ]);
        cpu.step().unwrap();
        cpu.step().unwrap();
        assert_eq!(cpu.read(0x0010), 0x42);
    }

    /// CPU → Bus → Ram: store is visible on every mirror and on `bus.ram` directly.
    #[test]
    fn cpu_bus_ram_store_mirrors() {
        let mut cpu = Cpu::new(vec![
            0xA9, 0x42, // LDA #$42
            0x85, 0x10, // STA $10
            0x00,       // BRK
        ]);
        cpu.step().unwrap();
        cpu.step().unwrap();

        assert_eq!(cpu.bus.read(0x0010), 0x42);
        assert_eq!(cpu.bus.read(0x0810), 0x42);
        assert_eq!(cpu.bus.read(0x1010), 0x42);
        assert_eq!(cpu.bus.read(0x1810), 0x42);
        assert_eq!(cpu.bus.ram.read(0x0010), 0x42);
    }

    /// Ram → Bus → CPU: seed memory via bus, then LDA Absolute reads it.
    #[test]
    fn cpu_bus_ram_load_from_seeded_ram() {
        let mut cpu = Cpu::new(vec![
            0xAD, 0x20, 0x00, // LDA $0020
            0x00,
        ]);
        cpu.bus.write(0x0020, 0x7E);

        cpu.step().unwrap();
        assert_eq!(cpu.a, 0x7E);
        assert!(!cpu.flag(FLAG_Z));
        assert!(!cpu.flag(FLAG_N));
    }

    /// Stack push/pull goes through Bus into Ram page `$01xx`.
    #[test]
    fn cpu_bus_ram_stack_pha_pla() {
        let mut cpu = Cpu::new(vec![
            0xA9, 0x55, // LDA #$55
            0x48,       // PHA
            0xA9, 0x00, // LDA #$00
            0x68,       // PLA
            0x00,
        ]);
        let sp_before = cpu.sp;

        cpu.step().unwrap(); // LDA
        cpu.step().unwrap(); // PHA
        assert_eq!(cpu.sp, sp_before.wrapping_sub(1));
        assert_eq!(cpu.bus.ram.read(0x0100 | sp_before as u16), 0x55);

        cpu.step().unwrap(); // LDA #$00
        assert_eq!(cpu.a, 0x00);
        cpu.step().unwrap(); // PLA
        assert_eq!(cpu.a, 0x55);
        assert_eq!(cpu.sp, sp_before);
    }

    /// Fetch + execute from PRG on Bus (`$8000`), using RAM for a zero-page store.
    #[test]
    fn cpu_bus_prg_fetch_and_ram_store() {
        let mut prg = vec![0; 0x4000];
        // Reset vector → $8000
        prg[0x3FFC] = 0x00;
        prg[0x3FFD] = 0x80;
        // $8000: LDA #$99; STA $10; BRK
        prg[0] = 0xA9;
        prg[1] = 0x99;
        prg[2] = 0x85;
        prg[3] = 0x10;
        prg[4] = 0x00;

        let mut cpu = Cpu::new(prg);
        assert_eq!(cpu.pc, 0x8000);
        assert!(cpu.bus.cartridge().is_some());
        assert_eq!(cpu.bus.read(0x8000), 0xA9);

        cpu.step().unwrap(); // LDA
        assert_eq!(cpu.a, 0x99);
        assert_eq!(cpu.pc, 0x8002);

        cpu.step().unwrap(); // STA $10
        assert_eq!(cpu.bus.ram.read(0x0010), 0x99);
        assert_eq!(cpu.bus.read(0x0810), 0x99);
    }

    #[test]
    fn from_rom_uses_cartridge_on_bus() {
        let rom = crate::cart::Loader::load("roms/super-mario-bros.nes").unwrap();
        let cpu = Cpu::from_rom(rom).unwrap();
        assert!(cpu.pc >= 0x8000);
        let cart = cpu.bus.cartridge().unwrap();
        assert_eq!(cart.mapper_id, 0);
        assert_eq!(cart.prg_rom().len(), 0x8000);
        assert_eq!(cart.chr_mem().len(), 0x2000);
    }

    #[test]
    fn smb_runs_with_ppu_and_advances_frames() {
        let rom = crate::cart::Loader::load("roms/super-mario-bros.nes").unwrap();
        let mut cpu = Cpu::from_rom(rom).unwrap();
        let start_frame = cpu.bus.ppu.frame;
        for _ in 0..50_000 {
            cpu.step().unwrap();
        }
        assert!(cpu.bus.ppu.frame > start_frame);
        assert!(cpu.cycles > 0);
    }
}
