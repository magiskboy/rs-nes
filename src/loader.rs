use std::{fs::File, io::Read};

use crate::error::NesError;

pub struct Loader;

#[derive(Debug)]
pub struct NesHeader {
    pub prg_size: u8,
    pub chr_size: u8,

    // https://www.nesdev.org/wiki/INES#Flags_6
    pub ntb_arrangement: NametableArrangement,
    pub battery_backed: bool,
    pub has_trainer: bool,
    pub alt_ntb_layout: bool,
    // pub lower_nybble: u8,

    //https://www.nesdev.org/wiki/INES#Flags_7
    pub vs_unisystem: bool,
    pub play_choice_10: bool,
    // not implement nes 2.0 format at this time
    // pub nes_20: bool,
    // pub upper_nybble: u8,

    // pub prg_ram_size: u8,

    // pub tv_system: TvSystem,
    //https://www.nesdev.org/wiki/INES#Flags_10
    //Not implemented
}

#[derive(Debug)]
pub enum TvSystem {
    NTSC,
    PAL,
}

#[derive(Debug)]
pub enum NametableArrangement {
    Vertical,
    Horizontal,
}

impl Loader {
    pub fn load(path: &str) -> Result<NesHeader, NesError> {
        let map_err = |_| NesError::GenericError;
        let mut file = File::open(path).map_err(map_err)?;
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).map_err(map_err)?;

        let format = &bytes[0..4];
        if !matches!(format, [0x4e, 0x45, 0x53, 0x1a]) {
            println!("Not match format signature, bytes {:?}", format);
            return Err(NesError::GenericError);
        }

        let prg_size = bytes[4];
        let chr_size = bytes[5];

        let flag6 = bytes[6];
        let ntb_arrangement = if ((flag6 >> 0) & 1) == 0 {
            NametableArrangement::Vertical
        } else {
            NametableArrangement::Horizontal
        };
        let battery_backed = (flag6 >> 1) & 1 == 1;
        let has_trainer = (flag6 >> 2) & 1 == 1;
        let alt_ntb_layout = (flag6 >> 3) & 1 == 1;

        let flag7 = bytes[7];
        let vs_unisystem = (flag7 >> 0) & 1 == 1;
        let play_choice_10 = (flag7 >> 1) & 1 == 1;

        let header = NesHeader {
            prg_size,
            chr_size,
            ntb_arrangement,
            battery_backed,
            has_trainer,
            alt_ntb_layout,
            vs_unisystem,
            play_choice_10,
        };
        println!("{:?}", header);
        Ok(header)
    }
}
