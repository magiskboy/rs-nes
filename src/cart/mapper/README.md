# NES mappers

How to add a board:

1. Create `src/cart/mapper/<name>.rs` with bank / IRQ logic
2. Add a variant to `MapperImpl` in `mod.rs`
3. Wire it in `MapperImpl::from_id` and `mapper_supported`
4. Move the row from **Not supported** → **Supported** below

Reference: [nesdev Mapper](https://www.nesdev.org/wiki/Mapper), [iNES Mapper List](https://www.nesdev.org/wiki/Category:INES_Mappers).

## Supported

| iNES | Board / chip | Module | Example games |
|------|----------------|--------|----------------|
| 0 | NROM | `nrom.rs` | Super Mario Bros. |
| 1 | MMC1 / SxROM | `mmc1.rs` | Zelda, Metroid |
| 2 | UxROM | `uxrom.rs` | Contra, Mega Man |
| 3 | CNROM | `cnrom.rs` | early CHR-bank titles |
| 4 | MMC3 / TxROM | `mmc3.rs` | Super Mario Bros. 3 |
| 5 | MMC5 / ExROM | `mmc5.rs` | Castlevania III |
| 7 | AxROM | `axrom.rs` | Battletoads |
| 71 | Camerica / Codemasters | `camerica.rs` | Micro Machines, Fire Hawk |

## Not supported

Prioritized backlog of boards worth adding (common commercial / well-documented). Cross off when implemented.

### High priority (popular titles)

| iNES | Board / chip | Notes / example games |
|------|----------------|------------------------|
| 9 | MMC2 / PxROM | Punch-Out!! |
| 10 | MMC4 / FxROM | Fire Emblem, Famicom Wars |
| 66 | GxROM / GNROM | Super Mario Bros. + Duck Hunt |
| 34 | BNROM / NINA-001 | Deadly Towers, Impossible Mission II |

### Konami VRC family

| iNES | Board / chip | Notes / example games |
|------|----------------|------------------------|
| 21 | VRC4a/c | Wai Wai World 2 |
| 22 | VRC2a | TwinBee 3 |
| 23 | VRC2b / VRC4e | Contra (J), Bio Miracle |
| 24 | VRC6a | Akumajou Densetsu |
| 25 | VRC4b/d | Gradius II |
| 26 | VRC6b | Madara, Esper Dream 2 |
| 73 | VRC3 | Salamander |
| 75 | VRC1 | TwinBee, Exciting Boxing |
| 85 | VRC7 | Lagrange Point (FM audio) |

### Namco / Sunsoft / others (mid priority)

| iNES | Board / chip | Notes / example games |
|------|----------------|------------------------|
| 18 | Jaleco SS88006 | Baseball 3, Moero!! |
| 19 | Namco 163 | Dig Dug II, Final Lap (wavetable audio) |
| 69 | FME-7 / Sunsoft 5B | Batman: Return of the Joker |
| 76 | Namco 109 | Digital Devil Story |
| 88 | Namco 118 | Dragon Spirit, Quinty |
| 95 | Namco 118 (alt) | Dragon Buster |
| 154 | Namco 118 (alt) | Devil Man |
| 206 | DxROM / Namco 108 | Gauntlet, Karnov |

### Simple discrete / unlicensed

| iNES | Board / chip | Notes / example games |
|------|----------------|------------------------|
| 11 | Color Dreams | many unlicensed |
| 13 | CPROM | Videomation |
| 38 | Crime Busters | Bit Corp. |
| 78 | Irem 74HC161 | Holy Diver, Uchuusen |
| 79 | NINA-003/006 | American Video titles |
| 87 | Jaleco/Konami | City Connection |
| 113 | NINA-06 / HES | multicarts |
| 140 | Jaleco JF-11/14 | Metal Slader (proto boards vary) |
| 184 | Sunsoft-1 | Atlantis no Nazo |

### Advanced / lower priority

Needs more PPU/APU hooks (IRQ variants, expansion audio, nametable tricks):

| iNES | Board / chip | Notes |
|------|----------------|--------|
| 5 | MMC5 extras | vertical split + MMC5 audio still incomplete in `mmc5.rs` |
| 16 / 153 / 157 / 159 | Bandai FCG / LZ93 | EEPROM, IRQ |
| 80 / 82 / 207 | Taito X1-005 / X1-017 | |
| 86 | Jaleco JF-13 | early ADPCM-ish |
| 90 / 209 / 211 | JY Company | complex ASIC |
| 118 | MMC3 TLSROM/TKSROM | MMC3 + one-screen via CHR A17 |
| 119 | MMC3 TQROM | MMC3 + CHR-ROM/RAM mix |
| 158 | Tengen 800037 | MMC3-like |
| 189 | TXC / MMC3 variant | Thunder Warrior |
| 210 | Namco 175/340 | simpler 163 without IRQs/audio |

### Everything else

Any iNES number **not** listed under **Supported** returns `NesError::UnsupportedMapper`. Pirate multicarts, modern homebrew ASICs, and obscure variants (e.g. 227, 228, 268, …) stay out of scope until a concrete ROM needs them.

## Quick checklist when adding one

- [ ] Banking (PRG ± CHR) matches nesdev for the games you care about
- [ ] Mirroring / four-screen / mapper-controlled NT if required
- [ ] IRQ path if the board has one (A12, scanline, CPU-cycle, …)
- [ ] PRG-RAM / battery size in `prg_ram_size`
- [ ] Unit test for bank select (and IRQ if applicable)
- [ ] Update this README (Supported + remove from Not supported)
