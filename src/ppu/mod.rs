pub mod palette;

pub use palette::{
    nes_rgb, put_pixel, BYTES_PER_PIXEL, FRAMEBUFFER_SIZE, NES_PALETTE,
    SCREEN_HEIGHT, SCREEN_WIDTH,
};

use crate::clock::Region;
use crate::cart::Mirroring;
use crate::cart::Cartridge;
use palette::{self as video};

/// Minimum A12-low duration (PPU dots) before a rising edge clocks MMC3.
const A12_FILTER_DOTS: u16 = 16;

/// One sprite slot for the current scanline (secondary OAM entry).
#[derive(Clone, Copy, Default)]
struct SpriteSlot {
    x: u8,
    attr: u8,
    tile: u8,
    /// Row within the sprite (0..height-1) before vertical flip is applied.
    row: u8,
    /// Pattern low / high for this row (already flipped horizontally if needed).
    pattern_lo: u8,
    pattern_hi: u8,
    /// True if this slot came from OAM sprite 0.
    is_sprite0: bool,
    /// False = empty secondary-OAM slot; hardware still fetches tile `$FF`.
    in_range: bool,
}

/// Picture Processing Unit (2C02 / 2C07) — registers, VRAM, rendering, VBlank/NMI.
///
/// Scanline layout follows [`Region`] (NTSC 262 / PAL 312). Visible dots write
/// RGB into [`Ppu::framebuffer`].
pub struct Ppu {
    // --- CPU-visible registers / latch state ---
    ctrl: u8,
    mask: u8,
    status: u8,
    oam_addr: u8,
    /// Loopy V (current VRAM address).
    v: u16,
    /// Loopy T (temporary VRAM address).
    t: u16,
    /// Fine X scroll (3 bits).
    fine_x: u8,
    /// First/second write toggle for $2005/$2006.
    write_toggle: bool,
    /// Buffered value returned on first $2007 read.
    data_buffer: u8,

    // --- Memory ---
    pub oam: [u8; 256],
    /// 4 KiB CIRAM — four-screen uses all; H/V/single use the first 2 KiB.
    nametables: [u8; 0x1000],
    palette: [u8; 32],

    // --- BG pipeline ---
    bg_shift_lo: u16,
    bg_shift_hi: u16,
    bg_attr_shift_lo: u16,
    bg_attr_shift_hi: u16,
    bg_next_tile: u8,
    bg_next_attr: u8,
    bg_next_lo: u8,
    bg_next_hi: u8,

    // --- Sprites ---
    sprites: [SpriteSlot; 8],
    sprite_count: u8,
    /// Sprites prepared for the *next* scanline during evaluation.
    next_sprites: [SpriteSlot; 8],
    next_sprite_count: u8,
    sprite_zero_in_line: bool,

    // --- Output ---
    framebuffer: [u8; FRAMEBUFFER_SIZE],

    // --- Timing ---
    region: Region,
    pub cycle: u16,
    pub scanline: i16,
    pub frame: u64,
    nmi_pending: bool,
    odd_frame: bool,
    /// Previous CHR address A12 — rising edge clocks MMC3 IRQ.
    last_chr_a12: bool,
    /// PPU dots A12 has stayed low; MMC3 ignores rises until this reaches
    /// [`A12_FILTER_DOTS`] (nesdev ≈ 10–16).
    a12_low_cycles: u16,
}

impl Ppu {
    pub fn new() -> Self {
        Self::with_region(Region::Ntsc)
    }

    pub fn with_region(region: Region) -> Self {
        Self {
            ctrl: 0,
            mask: 0,
            status: 0,
            oam_addr: 0,
            v: 0,
            t: 0,
            fine_x: 0,
            write_toggle: false,
            data_buffer: 0,
            oam: [0; 256],
            nametables: [0; 0x1000],
            palette: [0; 32],
            bg_shift_lo: 0,
            bg_shift_hi: 0,
            bg_attr_shift_lo: 0,
            bg_attr_shift_hi: 0,
            bg_next_tile: 0,
            bg_next_attr: 0,
            bg_next_lo: 0,
            bg_next_hi: 0,
            sprites: [SpriteSlot::default(); 8],
            sprite_count: 0,
            next_sprites: [SpriteSlot::default(); 8],
            next_sprite_count: 0,
            sprite_zero_in_line: false,
            framebuffer: [0; FRAMEBUFFER_SIZE],
            region,
            cycle: 0,
            scanline: -1,
            frame: 0,
            nmi_pending: false,
            odd_frame: false,
            last_chr_a12: false,
            a12_low_cycles: 0,
        }
    }

    pub fn region(&self) -> Region {
        self.region
    }

    pub fn set_region(&mut self, region: Region) {
        self.region = region;
    }

    pub fn reset_scanline_timing(&mut self) {
        self.cycle = 0;
        self.scanline = 0;
        self.nmi_pending = false;
    }

    /// Packed RGB framebuffer (`256×240×3`).
    pub fn framebuffer(&self) -> &[u8] {
        &self.framebuffer
    }

    fn nmi_enabled(&self) -> bool {
        self.ctrl & 0x80 != 0
    }

    fn vram_increment(&self) -> u16 {
        if self.ctrl & 0x04 != 0 {
            32
        } else {
            1
        }
    }

    pub fn cpu_read(&mut self, reg: u16, cart: &mut Option<Cartridge>) -> u8 {
        match reg & 7 {
            0 | 1 | 3 | 5 | 6 => 0,
            2 => {
                let value = (self.status & 0xE0) | (self.data_buffer & 0x1F);
                self.status &= !0x80;
                self.write_toggle = false;
                value
            }
            4 => self.oam[self.oam_addr as usize],
            7 => self.read_data(cart),
            _ => 0,
        }
    }

    pub fn cpu_write(&mut self, reg: u16, value: u8, cart: &mut Option<Cartridge>) {
        match reg & 7 {
            0 => {
                let was_off = !self.nmi_enabled();
                self.ctrl = value;
                self.t = (self.t & 0xF3FF) | ((value as u16 & 0x03) << 10);
                if was_off && self.nmi_enabled() && self.status & 0x80 != 0 {
                    self.nmi_pending = true;
                }
            }
            1 => self.mask = value,
            2 => {}
            3 => self.oam_addr = value,
            4 => {
                self.oam[self.oam_addr as usize] = value;
                self.oam_addr = self.oam_addr.wrapping_add(1);
            }
            5 => {
                if !self.write_toggle {
                    self.fine_x = value & 0x07;
                    self.t = (self.t & 0xFFE0) | ((value as u16) >> 3);
                } else {
                    self.t = (self.t & 0x8FFF) | ((value as u16 & 0x07) << 12);
                    self.t = (self.t & 0xFC1F) | ((value as u16 & 0xF8) << 2);
                }
                self.write_toggle = !self.write_toggle;
            }
            6 => {
                if !self.write_toggle {
                    self.t = (self.t & 0x00FF) | (((value as u16) & 0x3F) << 8);
                } else {
                    self.t = (self.t & 0xFF00) | value as u16;
                    self.v = self.t;
                }
                self.write_toggle = !self.write_toggle;
            }
            7 => self.write_data(value, cart),
            _ => {}
        }
    }

    fn read_data(&mut self, cart: &mut Option<Cartridge>) -> u8 {
        let addr = self.v & 0x3FFF;
        let value = if addr >= 0x3F00 {
            self.read_palette(addr)
        } else {
            let buffered = self.data_buffer;
            self.data_buffer = self.read_ppu_memory(addr, cart);
            buffered
        };
        self.v = self.v.wrapping_add(self.vram_increment()) & 0x7FFF;
        value
    }

    fn write_data(&mut self, value: u8, cart: &mut Option<Cartridge>) {
        let addr = self.v & 0x3FFF;
        self.write_ppu_memory(addr, value, cart);
        self.v = self.v.wrapping_add(self.vram_increment()) & 0x7FFF;
    }

    fn read_ppu_memory(&mut self, addr: u16, cart: &mut Option<Cartridge>) -> u8 {
        let addr = addr & 0x3FFF;
        match addr {
            0x0000..=0x1FFF => cart.as_mut().map(|c| c.ppu_read(addr)).unwrap_or(0),
            0x2000..=0x3EFF => {
                let mirroring = cart
                    .as_ref()
                    .map(|c| c.mirroring())
                    .unwrap_or(Mirroring::Horizontal);
                let index = Self::map_nametable_index(addr, mirroring);
                self.nametables[index]
            }
            0x3F00..=0x3FFF => self.read_palette(addr),
            _ => 0,
        }
    }

    fn write_ppu_memory(&mut self, addr: u16, value: u8, cart: &mut Option<Cartridge>) {
        let addr = addr & 0x3FFF;
        match addr {
            0x0000..=0x1FFF => {
                if let Some(c) = cart.as_mut() {
                    c.ppu_write(addr, value);
                }
            }
            0x2000..=0x3EFF => {
                let mirroring = cart
                    .as_ref()
                    .map(|c| c.mirroring())
                    .unwrap_or(Mirroring::Horizontal);
                let index = Self::map_nametable_index(addr, mirroring);
                self.nametables[index] = value;
            }
            0x3F00..=0x3FFF => self.write_palette(addr, value),
            _ => {}
        }
    }

    fn map_nametable_index(addr: u16, mirroring: Mirroring) -> usize {
        let a = (addr - 0x2000) & 0x0FFF;
        let mirrored = match mirroring {
            Mirroring::Horizontal => (a & 0x03FF) | ((a & 0x0800) >> 1),
            Mirroring::Vertical => a & 0x07FF,
            Mirroring::FourScreen => a,
            Mirroring::SingleScreenLower => a & 0x03FF,
            Mirroring::SingleScreenUpper => 0x0400 | (a & 0x03FF),
        };
        mirrored as usize
    }

    fn read_palette(&self, addr: u16) -> u8 {
        let mut index = (addr & 0x1F) as usize;
        if index >= 0x10 && index & 0x03 == 0 {
            index -= 0x10;
        }
        self.palette[index] & 0x3F
    }

    fn write_palette(&mut self, addr: u16, value: u8) {
        let mut index = (addr & 0x1F) as usize;
        if index >= 0x10 && index & 0x03 == 0 {
            index -= 0x10;
        }
        self.palette[index] = value & 0x3F;
    }

    fn bg_enabled(&self) -> bool {
        self.mask & 0x08 != 0
    }

    fn spr_enabled(&self) -> bool {
        self.mask & 0x10 != 0
    }

    fn rendering_enabled(&self) -> bool {
        self.mask & 0x18 != 0
    }

    fn sprite_height(&self) -> i16 {
        if self.ctrl & 0x20 != 0 {
            16
        } else {
            8
        }
    }

    // ─── Loopy scroll helpers ───────────────────────────────────────────────

    /// Coarse X++ with nametable horizontal wrap (nesdev PPU scrolling).
    pub fn increment_x(&mut self) {
        if self.v & 0x001F == 31 {
            self.v &= !0x001F;
            self.v ^= 0x0400;
        } else {
            self.v += 1;
        }
    }

    /// Fine Y++ / coarse Y++ with nametable vertical wrap.
    pub fn increment_y(&mut self) {
        if self.v & 0x7000 != 0x7000 {
            self.v += 0x1000;
        } else {
            self.v &= !0x7000;
            let mut y = (self.v & 0x03E0) >> 5;
            if y == 29 {
                y = 0;
                self.v ^= 0x0800;
            } else if y == 31 {
                y = 0;
            } else {
                y += 1;
            }
            self.v = (self.v & !0x03E0) | (y << 5);
        }
    }

    /// Copy horizontal bits of t → v (cycle 257).
    pub fn copy_x(&mut self) {
        self.v = (self.v & !0x041F) | (self.t & 0x041F);
    }

    /// Copy vertical bits of t → v (pre-render 280–304).
    pub fn copy_y(&mut self) {
        self.v = (self.v & !0x7BE0) | (self.t & 0x7BE0);
    }

    // ─── Background fetch / shift ───────────────────────────────────────────

    fn load_bg_shifters(&mut self) {
        self.bg_shift_lo = (self.bg_shift_lo & 0xFF00) | self.bg_next_lo as u16;
        self.bg_shift_hi = (self.bg_shift_hi & 0xFF00) | self.bg_next_hi as u16;
        let al = if self.bg_next_attr & 0x01 != 0 {
            0xFF
        } else {
            0x00
        };
        let ah = if self.bg_next_attr & 0x02 != 0 {
            0xFF
        } else {
            0x00
        };
        self.bg_attr_shift_lo = (self.bg_attr_shift_lo & 0xFF00) | al;
        self.bg_attr_shift_hi = (self.bg_attr_shift_hi & 0xFF00) | ah;
    }

    fn shift_bg(&mut self) {
        if self.bg_enabled() {
            self.bg_shift_lo <<= 1;
            self.bg_shift_hi <<= 1;
            self.bg_attr_shift_lo <<= 1;
            self.bg_attr_shift_hi <<= 1;
        }
    }

    /// Drive MMC3 A12 edge detection from a PPU bus address (CHR or nametable).
    fn observe_ppu_a12(&mut self, addr: u16, cart: &mut Option<Cartridge>) {
        let a12 = addr & 0x1000 != 0;
        if a12 && !self.last_chr_a12 && self.a12_low_cycles >= A12_FILTER_DOTS {
            if let Some(c) = cart.as_mut() {
                c.on_ppu_a12_rise();
            }
        }
        if a12 {
            self.a12_low_cycles = 0;
        }
        self.last_chr_a12 = a12;
    }

    /// CHR fetch that also tracks A12 rising edges for mapper IRQs (MMC3).
    fn chr_fetch(&mut self, addr: u16, cart: &mut Option<Cartridge>) -> u8 {
        self.observe_ppu_a12(addr, cart);
        cart.as_ref().map(|c| c.ppu_read(addr)).unwrap_or(0)
    }

    fn fetch_bg(&mut self, cart: &mut Option<Cartridge>) {
        // Within each 8-dot window: 0=NT, 2=AT, 4=lo, 6=hi, 7=inc hori(v)+reload.
        match self.cycle % 8 {
            1 => {
                let addr = 0x2000 | (self.v & 0x0FFF);
                self.bg_next_tile = self.read_ppu_memory(addr, cart);
            }
            3 => {
                let addr = 0x23C0
                    | (self.v & 0x0C00)
                    | ((self.v >> 4) & 0x38)
                    | ((self.v >> 2) & 0x07);
                let mut attr = self.read_ppu_memory(addr, cart);
                if self.v & 0x40 != 0 {
                    attr >>= 4;
                }
                if self.v & 0x02 != 0 {
                    attr >>= 2;
                }
                self.bg_next_attr = attr & 0x03;
            }
            5 => {
                let fine_y = (self.v >> 12) & 0x7;
                let table = if self.ctrl & 0x10 != 0 { 0x1000 } else { 0 };
                let addr = table | ((self.bg_next_tile as u16) << 4) | fine_y;
                self.bg_next_lo = self.chr_fetch(addr, cart);
            }
            7 => {
                let fine_y = (self.v >> 12) & 0x7;
                let table = if self.ctrl & 0x10 != 0 { 0x1000 } else { 0 };
                let addr = table | ((self.bg_next_tile as u16) << 4) | fine_y | 8;
                self.bg_next_hi = self.chr_fetch(addr, cart);
            }
            0 => {
                // End of 8-dot window (cycles 8,16,…256 and 328,336).
                self.load_bg_shifters();
                if self.rendering_enabled() {
                    self.increment_x();
                }
            }
            _ => {}
        }
    }

    fn bg_pixel_bits(&self) -> (u8, u8) {
        if !self.bg_enabled() {
            return (0, 0);
        }
        let mux = 0x8000u16 >> self.fine_x;
        let p0 = if self.bg_shift_lo & mux != 0 { 1 } else { 0 };
        let p1 = if self.bg_shift_hi & mux != 0 { 1 } else { 0 };
        let a0 = if self.bg_attr_shift_lo & mux != 0 {
            1
        } else {
            0
        };
        let a1 = if self.bg_attr_shift_hi & mux != 0 {
            1
        } else {
            0
        };
        ((p1 << 1) | p0, (a1 << 1) | a0)
    }

    // ─── Sprites ────────────────────────────────────────────────────────────

    /// OAM scan only — pattern bytes are fetched later in cycles 257–320 so
    /// empty slots still produce CHR A12 edges for MMC3 (SMB3 status bar).
    fn evaluate_sprites_for_next_line(&mut self) {
        self.next_sprites = [SpriteSlot::default(); 8];
        self.next_sprite_count = 0;
        let mut overflow = false;
        let height = self.sprite_height();
        let prerender = self.region.prerender_scanline();
        // OAM Y is "top minus 1"; sprite appears on scanlines [Y+1, Y+height].
        let next_sl = if self.scanline == prerender {
            0
        } else {
            self.scanline + 1
        };

        for i in 0..64 {
            let base = i * 4;
            let y = self.oam[base] as i16;
            let diff = next_sl - y;
            if diff < 1 || diff > height {
                continue;
            }
            let row = (diff - 1) as u8;
            if self.next_sprite_count < 8 {
                let slot = SpriteSlot {
                    x: self.oam[base + 3],
                    attr: self.oam[base + 2],
                    tile: self.oam[base + 1],
                    row,
                    pattern_lo: 0,
                    pattern_hi: 0,
                    is_sprite0: i == 0,
                    in_range: true,
                };
                self.next_sprites[self.next_sprite_count as usize] = slot;
                self.next_sprite_count += 1;
            } else {
                overflow = true;
            }
        }
        if overflow && self.spr_enabled() {
            self.status |= 0x20;
        }
    }

    /// Pattern-table addresses for one secondary-OAM slot (lo, hi plane).
    fn sprite_pattern_addrs(&self, slot: &SpriteSlot) -> (u16, u16) {
        let height = self.sprite_height() as u8;
        let (tile, mut row, attr) = if slot.in_range {
            (slot.tile, slot.row, slot.attr)
        } else {
            // Hardware leaves cleared secondary OAM as $FF and still fetches it.
            (0xFF, 0, 0)
        };

        if attr & 0x80 != 0 {
            row = height - 1 - row;
        }

        let (table_base, tile_index, fine_row) = if height == 16 {
            let top = tile & 0xFE;
            let bank = (tile as u16 & 1) * 0x1000;
            let (idx, r) = if row < 8 {
                (top, row)
            } else {
                (top.wrapping_add(1), row - 8)
            };
            (bank, idx, r)
        } else {
            let bank = if self.ctrl & 0x08 != 0 { 0x1000 } else { 0 };
            (bank, tile, row)
        };

        let addr = table_base | ((tile_index as u16) << 4) | fine_row as u16;
        (addr, addr | 8)
    }

    /// Cycles 257–320: 8 sprites × 8 dots (garbage NT + pattern lo/hi).
    fn fetch_sprite_slot(&mut self, cart: &mut Option<Cartridge>) {
        if !(257..=320).contains(&self.cycle) {
            return;
        }
        let rel = self.cycle - 257;
        let slot_idx = (rel / 8) as usize;
        let phase = rel % 8;
        let slot = self.sprites[slot_idx];

        match phase {
            // Garbage nametable fetches — pull A12 low between pattern fetches.
            1 | 3 => {
                let addr = 0x2000 | (self.v & 0x0FFF);
                self.observe_ppu_a12(addr, cart);
            }
            5 => {
                let (lo_addr, _) = self.sprite_pattern_addrs(&slot);
                let mut lo = self.chr_fetch(lo_addr, cart);
                if slot.in_range && slot.attr & 0x40 != 0 {
                    lo = lo.reverse_bits();
                }
                self.sprites[slot_idx].pattern_lo = lo;
            }
            7 => {
                let (_, hi_addr) = self.sprite_pattern_addrs(&slot);
                let mut hi = self.chr_fetch(hi_addr, cart);
                if slot.in_range && slot.attr & 0x40 != 0 {
                    hi = hi.reverse_bits();
                }
                self.sprites[slot_idx].pattern_hi = hi;
            }
            _ => {}
        }
    }

    fn sprite_pixel_at(&self, x: u16) -> (u8, u8, bool, bool) {
        // Returns (pixel 0-3, palette 0-3, behind_bg, is_sprite0)
        if !self.spr_enabled() {
            return (0, 0, false, false);
        }
        for i in 0..self.sprite_count as usize {
            let s = self.sprites[i];
            let dx = x as i16 - s.x as i16;
            if !(0..8).contains(&dx) {
                continue;
            }
            let bit = 7 - dx as u8;
            let p0 = (s.pattern_lo >> bit) & 1;
            let p1 = (s.pattern_hi >> bit) & 1;
            let pix = (p1 << 1) | p0;
            if pix == 0 {
                continue; // transparent — try lower-priority sprites
            }
            let palette = s.attr & 0x03;
            let behind = s.attr & 0x20 != 0;
            return (pix, palette, behind, s.is_sprite0);
        }
        (0, 0, false, false)
    }

    // ─── Pixel compose ──────────────────────────────────────────────────────

    fn render_pixel(&mut self) {
        let last_visible = if self.region == Region::Pal { 238 } else { 239 };
        if self.scanline < 0 || self.scanline > last_visible {
            return;
        }
        if self.cycle < 1 || self.cycle > 256 {
            return;
        }

        let x = (self.cycle - 1) as usize;
        let y = self.scanline as usize;
        if y >= SCREEN_HEIGHT || x >= SCREEN_WIDTH {
            return;
        }

        let (mut bg_pixel, bg_palette) = self.bg_pixel_bits();

        // Left clip BG
        if x < 8 && self.mask & 0x02 == 0 {
            bg_pixel = 0;
        }

        let (mut spr_pixel, spr_palette, spr_behind, is_s0) = self.sprite_pixel_at(x as u16);
        if x < 8 && self.mask & 0x04 == 0 {
            spr_pixel = 0;
        }

        // Sprite 0 hit
        if is_s0
            && spr_pixel != 0
            && bg_pixel != 0
            && x != 255
            && self.bg_enabled()
            && self.spr_enabled()
            && self.status & 0x40 == 0
        {
            self.status |= 0x40;
        }

        let (pixel, palette_high) = match (bg_pixel, spr_pixel) {
            (0, 0) => (0u8, 0u8),
            (0, s) => (s, spr_palette | 0x04),
            (b, 0) => (b, bg_palette),
            (b, s) => {
                if spr_behind {
                    (b, bg_palette)
                } else {
                    (s, spr_palette | 0x04)
                }
            }
        };

        let mut color = if pixel == 0 {
            self.read_palette(0x3F00)
        } else {
            self.read_palette(0x3F00 | ((palette_high as u16) << 2) | pixel as u16)
        };
        // PPUMASK bit 0 — grayscale (keep value bits only).
        if self.mask & 0x01 != 0 {
            color &= 0x30;
        }
        let mut rgb = video::nes_rgb(color);
        // PPUMASK bits 5–7 — color emphasis (PAL swaps red/green bits).
        let (emph_r, emph_g, emph_b) = match self.region {
            Region::Ntsc => (
                self.mask & 0x20 != 0,
                self.mask & 0x40 != 0,
                self.mask & 0x80 != 0,
            ),
            Region::Pal => (
                self.mask & 0x40 != 0,
                self.mask & 0x20 != 0,
                self.mask & 0x80 != 0,
            ),
        };
        if emph_r || emph_g || emph_b {
            const ATTEN: f32 = 0.75;
            if emph_r {
                rgb[1] = (rgb[1] as f32 * ATTEN) as u8;
                rgb[2] = (rgb[2] as f32 * ATTEN) as u8;
            }
            if emph_g {
                rgb[0] = (rgb[0] as f32 * ATTEN) as u8;
                rgb[2] = (rgb[2] as f32 * ATTEN) as u8;
            }
            if emph_b {
                rgb[0] = (rgb[0] as f32 * ATTEN) as u8;
                rgb[1] = (rgb[1] as f32 * ATTEN) as u8;
            }
        }
        video::put_pixel(&mut self.framebuffer, x, y, rgb);
    }

    /// Advance one PPU dot.
    pub fn tick(&mut self, cart: &mut Option<Cartridge>) {
        let vblank_sl = self.region.vblank_scanline();
        let prerender_sl = self.region.prerender_scanline();
        let last_visible = if self.region == Region::Pal { 238 } else { 239 };

        let rendering_line = (self.scanline >= 0 && self.scanline <= last_visible)
            || self.scanline == prerender_sl;

        if rendering_line && self.rendering_enabled() {
            // Visible pixel output (not on pre-render).
            if self.scanline >= 0 && self.scanline <= last_visible {
                self.render_pixel();
            }

            // BG fetch windows: 1–256 and 321–336.
            if (self.cycle >= 1 && self.cycle <= 256)
                || (self.cycle >= 321 && self.cycle <= 336)
            {
                self.shift_bg();
                self.fetch_bg(cart);
            }

            // End of visible dots: increment Y.
            if self.cycle == 256 {
                self.increment_y();
            }

            // Copy horizontal scroll; activate next-line sprites (patterns filled below).
            if self.cycle == 257 {
                self.copy_x();
                self.sprites = self.next_sprites;
                self.sprite_count = self.next_sprite_count;
                self.sprite_zero_in_line = self.sprites.iter().any(|s| s.is_sprite0);
            }

            // Pre-render: repeatedly copy vertical bits.
            if self.scanline == prerender_sl && self.cycle >= 280 && self.cycle <= 304 {
                self.copy_y();
            }

            // Evaluate sprites for the *next* scanline near end of line.
            // (Simplified: full OAM scan once at cycle 256; CHR fetch is 257–320.)
            if self.cycle == 256 && self.scanline < last_visible {
                self.evaluate_sprites_for_next_line();
            }
            // Pre-render prepares sprites for scanline 0.
            if self.cycle == 256 && self.scanline == prerender_sl {
                self.evaluate_sprites_for_next_line();
            }

            // Sprite pattern fetches (all 8 slots, including empty → tile $FF).
            if self.cycle >= 257 && self.cycle <= 320 {
                self.fetch_sprite_slot(cart);
            }
        }

        // MMC3 A12 filter: count consecutive dots with A12 low.
        if !self.last_chr_a12 {
            self.a12_low_cycles = self.a12_low_cycles.saturating_add(1);
        }

        // VBlank set
        if self.scanline == vblank_sl && self.cycle == 1 {
            self.status |= 0x80;
            if self.nmi_enabled() {
                self.nmi_pending = true;
            }
        }

        // Clear VBlank / sprite flags at pre-render cycle 1.
        if self.scanline == prerender_sl && self.cycle == 1 {
            self.status &= !0xE0;
            self.sprites = [SpriteSlot::default(); 8];
            self.sprite_count = 0;
        }

        self.cycle += 1;
        if self.cycle > 340 {
            self.cycle = 0;
            self.scanline += 1;
            if self.scanline > prerender_sl {
                self.scanline = 0;
                self.frame += 1;
                self.odd_frame = !self.odd_frame;
            }
        }

        if self.region.odd_frame_skip()
            && self.odd_frame
            && self.scanline == prerender_sl
            && self.cycle == 340
            && self.rendering_enabled()
        {
            self.cycle = 0;
            self.scanline = 0;
            self.frame += 1;
            self.odd_frame = !self.odd_frame;
        }
    }

    pub fn take_nmi(&mut self) -> bool {
        if self.nmi_pending {
            self.nmi_pending = false;
            true
        } else {
            false
        }
    }

    pub fn status(&self) -> u8 {
        self.status
    }

    pub fn ctrl(&self) -> u8 {
        self.ctrl
    }

    pub fn oam_addr(&self) -> u8 {
        self.oam_addr
    }

    pub fn oam_dma_write(&mut self, value: u8) {
        self.oam[self.oam_addr as usize] = value;
        self.oam_addr = self.oam_addr.wrapping_add(1);
    }

    /// Binary snapshot of PPU regs + VRAM/OAM/palette + timing (for savestates).
    pub fn export_state(&self, out: &mut Vec<u8>) {
        out.push(self.ctrl);
        out.push(self.mask);
        out.push(self.status);
        out.push(self.oam_addr);
        out.extend_from_slice(&self.v.to_le_bytes());
        out.extend_from_slice(&self.t.to_le_bytes());
        out.push(self.fine_x);
        out.push(u8::from(self.write_toggle));
        out.push(self.data_buffer);
        out.extend_from_slice(&self.oam);
        out.extend_from_slice(&self.nametables);
        out.extend_from_slice(&self.palette);
        out.extend_from_slice(&self.bg_shift_lo.to_le_bytes());
        out.extend_from_slice(&self.bg_shift_hi.to_le_bytes());
        out.extend_from_slice(&self.bg_attr_shift_lo.to_le_bytes());
        out.extend_from_slice(&self.bg_attr_shift_hi.to_le_bytes());
        out.push(self.bg_next_tile);
        out.push(self.bg_next_attr);
        out.push(self.bg_next_lo);
        out.push(self.bg_next_hi);
        for s in &self.sprites {
            write_sprite(out, s);
        }
        out.push(self.sprite_count);
        for s in &self.next_sprites {
            write_sprite(out, s);
        }
        out.push(self.next_sprite_count);
        out.push(u8::from(self.sprite_zero_in_line));
        // Skip framebuffer (rebuilt next frames); keep timing.
        out.extend_from_slice(&self.cycle.to_le_bytes());
        out.extend_from_slice(&self.scanline.to_le_bytes());
        out.extend_from_slice(&self.frame.to_le_bytes());
        out.push(u8::from(self.nmi_pending));
        out.push(u8::from(self.odd_frame));
        out.push(u8::from(self.last_chr_a12));
        out.extend_from_slice(&self.a12_low_cycles.to_le_bytes());
    }

    pub fn import_state(&mut self, data: &mut &[u8]) -> Result<(), String> {
        self.ctrl = take_u8(data)?;
        self.mask = take_u8(data)?;
        self.status = take_u8(data)?;
        self.oam_addr = take_u8(data)?;
        self.v = take_u16(data)?;
        self.t = take_u16(data)?;
        self.fine_x = take_u8(data)?;
        self.write_toggle = take_u8(data)? != 0;
        self.data_buffer = take_u8(data)?;
        take_into(data, &mut self.oam)?;
        take_into(data, &mut self.nametables)?;
        take_into(data, &mut self.palette)?;
        self.bg_shift_lo = take_u16(data)?;
        self.bg_shift_hi = take_u16(data)?;
        self.bg_attr_shift_lo = take_u16(data)?;
        self.bg_attr_shift_hi = take_u16(data)?;
        self.bg_next_tile = take_u8(data)?;
        self.bg_next_attr = take_u8(data)?;
        self.bg_next_lo = take_u8(data)?;
        self.bg_next_hi = take_u8(data)?;
        for s in &mut self.sprites {
            *s = read_sprite(data)?;
        }
        self.sprite_count = take_u8(data)?;
        for s in &mut self.next_sprites {
            *s = read_sprite(data)?;
        }
        self.next_sprite_count = take_u8(data)?;
        self.sprite_zero_in_line = take_u8(data)? != 0;
        self.cycle = take_u16(data)?;
        self.scanline = take_i16(data)?;
        self.frame = take_u64(data)?;
        self.nmi_pending = take_u8(data)? != 0;
        self.odd_frame = take_u8(data)? != 0;
        self.last_chr_a12 = take_u8(data)? != 0;
        self.a12_low_cycles = take_u16(data)?;
        Ok(())
    }
}

fn write_sprite(out: &mut Vec<u8>, s: &SpriteSlot) {
    out.push(s.x);
    out.push(s.attr);
    out.push(s.tile);
    out.push(s.row);
    out.push(s.pattern_lo);
    out.push(s.pattern_hi);
    out.push(u8::from(s.is_sprite0));
    out.push(u8::from(s.in_range));
}

fn read_sprite(data: &mut &[u8]) -> Result<SpriteSlot, String> {
    Ok(SpriteSlot {
        x: take_u8(data)?,
        attr: take_u8(data)?,
        tile: take_u8(data)?,
        row: take_u8(data)?,
        pattern_lo: take_u8(data)?,
        pattern_hi: take_u8(data)?,
        is_sprite0: take_u8(data)? != 0,
        in_range: take_u8(data)? != 0,
    })
}

fn take_u8(data: &mut &[u8]) -> Result<u8, String> {
    if data.is_empty() {
        return Err("ppu state truncated".into());
    }
    let v = data[0];
    *data = &data[1..];
    Ok(v)
}

fn take_u16(data: &mut &[u8]) -> Result<u16, String> {
    if data.len() < 2 {
        return Err("ppu state truncated".into());
    }
    let v = u16::from_le_bytes([data[0], data[1]]);
    *data = &data[2..];
    Ok(v)
}

fn take_i16(data: &mut &[u8]) -> Result<i16, String> {
    Ok(take_u16(data)? as i16)
}

fn take_u64(data: &mut &[u8]) -> Result<u64, String> {
    if data.len() < 8 {
        return Err("ppu state truncated".into());
    }
    let mut b = [0u8; 8];
    b.copy_from_slice(&data[..8]);
    *data = &data[8..];
    Ok(u64::from_le_bytes(b))
}

fn take_into(data: &mut &[u8], dst: &mut [u8]) -> Result<(), String> {
    if data.len() < dst.len() {
        return Err("ppu state truncated".into());
    }
    dst.copy_from_slice(&data[..dst.len()]);
    *data = &data[dst.len()..];
    Ok(())
}

impl Default for Ppu {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cart::Cartridge;

    #[test]
    fn vblank_sets_status_and_nmi() {
        let mut ppu = Ppu::new();
        ppu.ctrl = 0x80;
        ppu.scanline = 241;
        ppu.cycle = 1;
        ppu.tick(&mut None);
        assert!(ppu.status & 0x80 != 0);
        assert!(ppu.take_nmi());
    }

    #[test]
    fn pal_vblank_at_scanline_240() {
        let mut ppu = Ppu::with_region(Region::Pal);
        ppu.ctrl = 0x80;
        ppu.scanline = 240;
        ppu.cycle = 1;
        ppu.tick(&mut None);
        assert!(ppu.status & 0x80 != 0);
        assert!(ppu.take_nmi());
    }

    #[test]
    fn ppuaddr_ppudata_roundtrip_nametable() {
        let mut ppu = Ppu::new();
        let mut cart = Some(Cartridge::nrom(vec![0; 0x4000], vec![0; 0x2000]));
        ppu.cpu_write(6, 0x20, &mut cart);
        ppu.cpu_write(6, 0x00, &mut cart);
        ppu.cpu_write(7, 0x5A, &mut cart);
        ppu.cpu_write(6, 0x20, &mut cart);
        ppu.cpu_write(6, 0x00, &mut cart);
        let _ = ppu.cpu_read(7, &mut cart);
        let value = ppu.cpu_read(7, &mut cart);
        assert_eq!(value, 0x5A);
    }

    #[test]
    fn status_read_clears_vblank() {
        let mut ppu = Ppu::new();
        ppu.status |= 0x80;
        let mut none = None;
        let _ = ppu.cpu_read(2, &mut none);
        assert_eq!(ppu.status & 0x80, 0);
    }

    #[test]
    fn four_screen_uses_full_4kib() {
        assert_eq!(Ppu::map_nametable_index(0x2000, Mirroring::FourScreen), 0x000);
        assert_eq!(Ppu::map_nametable_index(0x2400, Mirroring::FourScreen), 0x400);
        assert_eq!(Ppu::map_nametable_index(0x2800, Mirroring::FourScreen), 0x800);
        assert_eq!(Ppu::map_nametable_index(0x2C00, Mirroring::FourScreen), 0xC00);
    }

    #[test]
    fn single_screen_maps_to_one_page() {
        assert_eq!(
            Ppu::map_nametable_index(0x2C00, Mirroring::SingleScreenLower),
            0x000
        );
        assert_eq!(
            Ppu::map_nametable_index(0x2000, Mirroring::SingleScreenUpper),
            0x400
        );
    }

    #[test]
    fn increment_x_wraps_nametable() {
        let mut ppu = Ppu::new();
        ppu.v = 0x001F; // coarse X = 31
        ppu.increment_x();
        assert_eq!(ppu.v & 0x001F, 0);
        assert_eq!(ppu.v & 0x0400, 0x0400);
    }

    #[test]
    fn renders_opaque_bg_pixel_to_framebuffer() {
        let mut ppu = Ppu::new();
        ppu.mask = 0x1E; // show BG+sprites, no left clip
        // Palette: colour 1 → index $01
        ppu.palette[1] = 0x01;
        // Tile 0, row 0: opaque bit at fine_x=0 → colour 1
        let mut chr = vec![0u8; 0x2000];
        chr[0] = 0x80; // lo plane
        chr[8] = 0x00; // hi plane → pixel = 1
        let cart = Cartridge::nrom(vec![0; 0x4000], chr);

        // Prefetch two tiles worth so shifters have data (cycles 321–336 style):
        // Manually load next tile data then shifters.
        ppu.bg_next_tile = 0;
        ppu.bg_next_attr = 0;
        ppu.bg_next_lo = 0x80;
        ppu.bg_next_hi = 0x00;
        ppu.load_bg_shifters();
        // Shift so bit 15 aligns with fine_x=0: after load, low byte is in bits 7:0.
        // Hardware keeps previous tile in high byte; for first pixel we need the
        // next tile in the high half — reload into high by shifting 8 times.
        for _ in 0..8 {
            ppu.shift_bg();
        }
        ppu.bg_next_lo = 0x80;
        ppu.bg_next_hi = 0x00;
        ppu.load_bg_shifters();
        for _ in 0..8 {
            ppu.shift_bg();
        }
        // Now opaque bits sit in low half; fine_x=0 samples bit 15 after 8 more?
        // Simpler path: set shift regs directly.
        ppu.bg_shift_lo = 0x8000;
        ppu.bg_shift_hi = 0x0000;
        ppu.bg_attr_shift_lo = 0;
        ppu.bg_attr_shift_hi = 0;
        ppu.fine_x = 0;
        ppu.scanline = 0;
        ppu.cycle = 1;
        ppu.render_pixel();

        let rgb = video::nes_rgb(0x01);
        assert_eq!(&ppu.framebuffer[0..3], &rgb);
        let _ = cart; // keep cart alive for CHR if needed later
    }

    #[test]
    fn sprite0_hit_sets_status_bit() {
        let mut ppu = Ppu::new();
        ppu.mask = 0x1E;
        ppu.oam[0] = 0;
        ppu.oam[1] = 0;
        ppu.oam[2] = 0;
        ppu.oam[3] = 0;

        let mut chr = vec![0u8; 0x2000];
        chr[0] = 0x80;
        chr[8] = 0x80;
        let cart = Cartridge::nrom(vec![0; 0x4000], chr);

        // Prepare sprite slot for scanline 0 and opaque BG.
        ppu.sprites[0] = SpriteSlot {
            x: 0,
            attr: 0,
            tile: 0,
            row: 0,
            pattern_lo: 0x80,
            pattern_hi: 0x80,
            is_sprite0: true,
            in_range: true,
        };
        ppu.sprite_count = 1;
        ppu.bg_shift_lo = 0x8000;
        ppu.bg_shift_hi = 0x8000;
        ppu.scanline = 0;
        ppu.cycle = 1;
        ppu.render_pixel();
        assert!(ppu.status & 0x40 != 0, "sprite 0 hit expected");
        let _ = cart;
    }

    /// Empty secondary-OAM slots must still CHR-fetch (tile `$FF`) so MMC3
    /// gets one A12 clock per scanline — required for SMB3's status bar split.
    #[test]
    fn dummy_sprite_fetches_clock_mmc3_a12() {
        use crate::cart::Loader;

        let mut bytes = vec![0u8; 16 + 0x8000 + 0x2000];
        bytes[0..4].copy_from_slice(b"NES\x1a");
        bytes[4] = 2; // 32 KiB PRG
        bytes[5] = 1; // 8 KiB CHR
        bytes[6] = 0x40; // mapper 4
        let rom = Loader::parse(&bytes).unwrap();
        let mut cart = Some(Cartridge::from_rom(rom).unwrap());
        cart.as_mut().unwrap().cpu_write(0xC000, 0); // latch = 0 → IRQ every clock
        cart.as_mut().unwrap().cpu_write(0xC001, 0); // reload
        cart.as_mut().unwrap().cpu_write(0xE001, 0); // enable

        let mut ppu = Ppu::new();
        ppu.ctrl = 0x08; // sprite pattern table $1000
        ppu.a12_low_cycles = A12_FILTER_DOTS;
        ppu.sprites = [SpriteSlot::default(); 8]; // all empty → fetch $FF
        ppu.sprite_count = 0;

        for cycle in 257..=320 {
            ppu.cycle = cycle;
            ppu.fetch_sprite_slot(&mut cart);
            if !ppu.last_chr_a12 {
                ppu.a12_low_cycles = ppu.a12_low_cycles.saturating_add(1);
            }
        }
        assert!(
            cart.as_ref().unwrap().irq_pending(),
            "expected MMC3 IRQ from dummy sprite pattern fetches"
        );
    }
}
