use crate::error::NesError;

/// Official 6502 mnemonic (see https://www.nesdev.org/wiki/Instruction_reference).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mnemonic {
    Adc,
    And,
    Asl,
    Bcc,
    Bcs,
    Beq,
    Bit,
    Bmi,
    Bne,
    Bpl,
    Brk,
    Bvc,
    Bvs,
    Clc,
    Cld,
    Cli,
    Clv,
    Cmp,
    Cpx,
    Cpy,
    Dec,
    Dex,
    Dey,
    Eor,
    Inc,
    Inx,
    Iny,
    Jmp,
    Jsr,
    Lda,
    Ldx,
    Ldy,
    Lsr,
    Nop,
    Ora,
    Pha,
    Php,
    Pla,
    Plp,
    Rol,
    Ror,
    Rti,
    Rts,
    Sbc,
    Sec,
    Sed,
    Sei,
    Sta,
    Stx,
    Sty,
    Tax,
    Tay,
    Tsx,
    Txa,
    Txs,
    Tya,
}

/// Addressing mode for an opcode variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressingMode {
    Implied,
    Accumulator,
    Immediate,
    ZeroPage,
    ZeroPageX,
    ZeroPageY,
    Absolute,
    AbsoluteX,
    AbsoluteY,
    Indirect,
    IndirectX,
    IndirectY,
    Relative,
}

/// Operand bytes decoded after the opcode (little-endian for 16-bit values).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operand {
    None,
    Byte(u8),
    Word(u16),
}

/// A fully decoded instruction at a given cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Instruction {
    pub mnemonic: Mnemonic,
    pub mode: AddressingMode,
    pub opcode: u8,
    pub operand: Operand,
}

/// Result of decoding one instruction: the instruction and the next cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodedOp {
    pub instruction: Instruction,
    pub next: usize,
}

/// Decode one instruction starting at `cursor` in `bytes`.
///
/// Returns the instruction (mnemonic + addressing mode + operand bytes) and
/// the cursor position of the first byte of the following instruction.
pub fn decode(cursor: usize, bytes: &[u8]) -> Result<DecodedOp, NesError> {
    let opcode = *bytes.get(cursor).ok_or(NesError::GenericError)?;
    let (mnemonic, mode, size) = opcode_info(opcode).ok_or(NesError::GenericError)?;
    let end = cursor
        .checked_add(size)
        .filter(|&e| e <= bytes.len())
        .ok_or(NesError::GenericError)?;

    let operand = match size {
        1 => Operand::None,
        2 => Operand::Byte(bytes[cursor + 1]),
        3 => {
            let lo = bytes[cursor + 1] as u16;
            let hi = bytes[cursor + 2] as u16;
            Operand::Word(lo | (hi << 8))
        }
        _ => unreachable!("official opcodes are 1–3 bytes"),
    };

    Ok(DecodedOp {
        instruction: Instruction {
            mnemonic,
            mode,
            opcode,
            operand,
        },
        next: end,
    })
}

/// Look up mnemonic, addressing mode, and total size (opcode + operands) for an official opcode.
fn opcode_info(opcode: u8) -> Option<(Mnemonic, AddressingMode, usize)> {
    use AddressingMode::*;
    use Mnemonic::*;

    Some(match opcode {
        // ADC
        0x69 => (Adc, Immediate, 2),
        0x65 => (Adc, ZeroPage, 2),
        0x75 => (Adc, ZeroPageX, 2),
        0x6D => (Adc, Absolute, 3),
        0x7D => (Adc, AbsoluteX, 3),
        0x79 => (Adc, AbsoluteY, 3),
        0x61 => (Adc, IndirectX, 2),
        0x71 => (Adc, IndirectY, 2),

        // AND
        0x29 => (And, Immediate, 2),
        0x25 => (And, ZeroPage, 2),
        0x35 => (And, ZeroPageX, 2),
        0x2D => (And, Absolute, 3),
        0x3D => (And, AbsoluteX, 3),
        0x39 => (And, AbsoluteY, 3),
        0x21 => (And, IndirectX, 2),
        0x31 => (And, IndirectY, 2),

        // ASL
        0x0A => (Asl, Accumulator, 1),
        0x06 => (Asl, ZeroPage, 2),
        0x16 => (Asl, ZeroPageX, 2),
        0x0E => (Asl, Absolute, 3),
        0x1E => (Asl, AbsoluteX, 3),

        // Branches
        0x90 => (Bcc, Relative, 2),
        0xB0 => (Bcs, Relative, 2),
        0xF0 => (Beq, Relative, 2),
        0x30 => (Bmi, Relative, 2),
        0xD0 => (Bne, Relative, 2),
        0x10 => (Bpl, Relative, 2),
        0x50 => (Bvc, Relative, 2),
        0x70 => (Bvs, Relative, 2),

        // BIT
        0x24 => (Bit, ZeroPage, 2),
        0x2C => (Bit, Absolute, 3),

        // BRK (1-byte implied; return address skips the following byte at runtime)
        0x00 => (Brk, Implied, 1),

        // Flags / stack / transfer / implied misc
        0x18 => (Clc, Implied, 1),
        0xD8 => (Cld, Implied, 1),
        0x58 => (Cli, Implied, 1),
        0xB8 => (Clv, Implied, 1),
        0xCA => (Dex, Implied, 1),
        0x88 => (Dey, Implied, 1),
        0xE8 => (Inx, Implied, 1),
        0xC8 => (Iny, Implied, 1),
        0xEA => (Nop, Implied, 1),
        0x48 => (Pha, Implied, 1),
        0x08 => (Php, Implied, 1),
        0x68 => (Pla, Implied, 1),
        0x28 => (Plp, Implied, 1),
        0x40 => (Rti, Implied, 1),
        0x60 => (Rts, Implied, 1),
        0x38 => (Sec, Implied, 1),
        0xF8 => (Sed, Implied, 1),
        0x78 => (Sei, Implied, 1),
        0xAA => (Tax, Implied, 1),
        0xA8 => (Tay, Implied, 1),
        0xBA => (Tsx, Implied, 1),
        0x8A => (Txa, Implied, 1),
        0x9A => (Txs, Implied, 1),
        0x98 => (Tya, Implied, 1),

        // CMP
        0xC9 => (Cmp, Immediate, 2),
        0xC5 => (Cmp, ZeroPage, 2),
        0xD5 => (Cmp, ZeroPageX, 2),
        0xCD => (Cmp, Absolute, 3),
        0xDD => (Cmp, AbsoluteX, 3),
        0xD9 => (Cmp, AbsoluteY, 3),
        0xC1 => (Cmp, IndirectX, 2),
        0xD1 => (Cmp, IndirectY, 2),

        // CPX / CPY
        0xE0 => (Cpx, Immediate, 2),
        0xE4 => (Cpx, ZeroPage, 2),
        0xEC => (Cpx, Absolute, 3),
        0xC0 => (Cpy, Immediate, 2),
        0xC4 => (Cpy, ZeroPage, 2),
        0xCC => (Cpy, Absolute, 3),

        // DEC / INC
        0xC6 => (Dec, ZeroPage, 2),
        0xD6 => (Dec, ZeroPageX, 2),
        0xCE => (Dec, Absolute, 3),
        0xDE => (Dec, AbsoluteX, 3),
        0xE6 => (Inc, ZeroPage, 2),
        0xF6 => (Inc, ZeroPageX, 2),
        0xEE => (Inc, Absolute, 3),
        0xFE => (Inc, AbsoluteX, 3),

        // EOR
        0x49 => (Eor, Immediate, 2),
        0x45 => (Eor, ZeroPage, 2),
        0x55 => (Eor, ZeroPageX, 2),
        0x4D => (Eor, Absolute, 3),
        0x5D => (Eor, AbsoluteX, 3),
        0x59 => (Eor, AbsoluteY, 3),
        0x41 => (Eor, IndirectX, 2),
        0x51 => (Eor, IndirectY, 2),

        // JMP / JSR
        0x4C => (Jmp, Absolute, 3),
        0x6C => (Jmp, Indirect, 3),
        0x20 => (Jsr, Absolute, 3),

        // LDA
        0xA9 => (Lda, Immediate, 2),
        0xA5 => (Lda, ZeroPage, 2),
        0xB5 => (Lda, ZeroPageX, 2),
        0xAD => (Lda, Absolute, 3),
        0xBD => (Lda, AbsoluteX, 3),
        0xB9 => (Lda, AbsoluteY, 3),
        0xA1 => (Lda, IndirectX, 2),
        0xB1 => (Lda, IndirectY, 2),

        // LDX
        0xA2 => (Ldx, Immediate, 2),
        0xA6 => (Ldx, ZeroPage, 2),
        0xB6 => (Ldx, ZeroPageY, 2),
        0xAE => (Ldx, Absolute, 3),
        0xBE => (Ldx, AbsoluteY, 3),

        // LDY
        0xA0 => (Ldy, Immediate, 2),
        0xA4 => (Ldy, ZeroPage, 2),
        0xB4 => (Ldy, ZeroPageX, 2),
        0xAC => (Ldy, Absolute, 3),
        0xBC => (Ldy, AbsoluteX, 3),

        // LSR
        0x4A => (Lsr, Accumulator, 1),
        0x46 => (Lsr, ZeroPage, 2),
        0x56 => (Lsr, ZeroPageX, 2),
        0x4E => (Lsr, Absolute, 3),
        0x5E => (Lsr, AbsoluteX, 3),

        // ORA
        0x09 => (Ora, Immediate, 2),
        0x05 => (Ora, ZeroPage, 2),
        0x15 => (Ora, ZeroPageX, 2),
        0x0D => (Ora, Absolute, 3),
        0x1D => (Ora, AbsoluteX, 3),
        0x19 => (Ora, AbsoluteY, 3),
        0x01 => (Ora, IndirectX, 2),
        0x11 => (Ora, IndirectY, 2),

        // ROL / ROR
        0x2A => (Rol, Accumulator, 1),
        0x26 => (Rol, ZeroPage, 2),
        0x36 => (Rol, ZeroPageX, 2),
        0x2E => (Rol, Absolute, 3),
        0x3E => (Rol, AbsoluteX, 3),
        0x6A => (Ror, Accumulator, 1),
        0x66 => (Ror, ZeroPage, 2),
        0x76 => (Ror, ZeroPageX, 2),
        0x6E => (Ror, Absolute, 3),
        0x7E => (Ror, AbsoluteX, 3),

        // SBC
        0xE9 => (Sbc, Immediate, 2),
        0xE5 => (Sbc, ZeroPage, 2),
        0xF5 => (Sbc, ZeroPageX, 2),
        0xED => (Sbc, Absolute, 3),
        0xFD => (Sbc, AbsoluteX, 3),
        0xF9 => (Sbc, AbsoluteY, 3),
        0xE1 => (Sbc, IndirectX, 2),
        0xF1 => (Sbc, IndirectY, 2),

        // STA / STX / STY
        0x85 => (Sta, ZeroPage, 2),
        0x95 => (Sta, ZeroPageX, 2),
        0x8D => (Sta, Absolute, 3),
        0x9D => (Sta, AbsoluteX, 3),
        0x99 => (Sta, AbsoluteY, 3),
        0x81 => (Sta, IndirectX, 2),
        0x91 => (Sta, IndirectY, 2),
        0x86 => (Stx, ZeroPage, 2),
        0x96 => (Stx, ZeroPageY, 2),
        0x8E => (Stx, Absolute, 3),
        0x84 => (Sty, ZeroPage, 2),
        0x94 => (Sty, ZeroPageX, 2),
        0x8C => (Sty, Absolute, 3),

        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_lda_immediate() {
        let bytes = [0xA9, 0x42];
        let decoded = decode(0, &bytes).unwrap();
        assert_eq!(decoded.instruction.mnemonic, Mnemonic::Lda);
        assert_eq!(decoded.instruction.mode, AddressingMode::Immediate);
        assert_eq!(decoded.instruction.operand, Operand::Byte(0x42));
        assert_eq!(decoded.next, 2);
    }

    #[test]
    fn decode_jmp_absolute() {
        let bytes = [0x4C, 0x34, 0x12];
        let decoded = decode(0, &bytes).unwrap();
        assert_eq!(decoded.instruction.mnemonic, Mnemonic::Jmp);
        assert_eq!(decoded.instruction.mode, AddressingMode::Absolute);
        assert_eq!(decoded.instruction.operand, Operand::Word(0x1234));
        assert_eq!(decoded.next, 3);
    }

    #[test]
    fn decode_implied_and_advance_cursor() {
        let bytes = [0xEA, 0xA9, 0x01];
        let first = decode(0, &bytes).unwrap();
        assert_eq!(first.instruction.mnemonic, Mnemonic::Nop);
        assert_eq!(first.next, 1);

        let second = decode(first.next, &bytes).unwrap();
        assert_eq!(second.instruction.mnemonic, Mnemonic::Lda);
        assert_eq!(second.instruction.operand, Operand::Byte(0x01));
        assert_eq!(second.next, 3);
    }

    #[test]
    fn reject_illegal_opcode() {
        assert!(decode(0, &[0x02]).is_err());
    }

    #[test]
    fn reject_truncated_operand() {
        assert!(decode(0, &[0xA9]).is_err());
    }
}
