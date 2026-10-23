//! Command-line arguments for the desktop binary.

use crate::emulator::video::VideoFilter;
use std::path::PathBuf;

#[derive(Debug, Clone, Default)]
pub struct Cli {
    /// Positional ROM file or folder.
    pub path: Option<PathBuf>,
    /// Extra ROM folders from `--roms`.
    pub roms_dirs: Vec<PathBuf>,
    pub skip_boot: bool,
    pub headless: bool,
    pub filter: Option<VideoFilter>,
    pub volume: Option<f32>,
    pub fullscreen: bool,
}

#[derive(Debug)]
pub enum ParseOutcome {
    Run(Cli),
    Help,
    Version,
}

pub fn parse<I, S>(args: I) -> Result<ParseOutcome, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut cli = Cli::default();
    let mut positional: Option<PathBuf> = None;
    let mut iter = args.into_iter().peekable();

    while let Some(raw) = iter.next() {
        let arg = raw.as_ref();
        match arg {
            "-h" | "--help" => return Ok(ParseOutcome::Help),
            "-V" | "--version" => return Ok(ParseOutcome::Version),
            "--headless" => cli.headless = true,
            "--skip-boot" => cli.skip_boot = true,
            "--fullscreen" => cli.fullscreen = true,
            "--roms" => {
                let dir = next_value(&mut iter, "--roms")?;
                cli.roms_dirs.push(PathBuf::from(dir));
            }
            a if a.starts_with("--roms=") => {
                cli.roms_dirs.push(PathBuf::from(&a["--roms=".len()..]));
            }
            "--filter" => {
                let name = next_value(&mut iter, "--filter")?;
                cli.filter = Some(parse_filter(&name)?);
            }
            a if a.starts_with("--filter=") => {
                cli.filter = Some(parse_filter(&a["--filter=".len()..])?);
            }
            "--volume" => {
                let v = next_value(&mut iter, "--volume")?;
                cli.volume = Some(parse_volume(&v)?);
            }
            a if a.starts_with("--volume=") => {
                cli.volume = Some(parse_volume(&a["--volume=".len()..])?);
            }
            a if a.starts_with('-') => {
                return Err(format!("unknown option: {a}\nTry `rs-nes --help`."));
            }
            path => {
                if positional.is_some() {
                    return Err(format!(
                        "unexpected argument: {path}\nOnly one ROM/folder path is allowed."
                    ));
                }
                positional = Some(PathBuf::from(path));
            }
        }
    }

    cli.path = positional;
    Ok(ParseOutcome::Run(cli))
}

pub fn print_help() {
    println!(
        "\
Rust NES — NES emulator with a small OS shell

Usage:
  rs-nes [OPTIONS] [ROM_OR_FOLDER]

Arguments:
  [ROM_OR_FOLDER]       .nes file or folder to add to the library

Options:
  --roms <DIR>          Add/scan a ROM folder (repeatable)
  --skip-boot           Skip the NES OS splash
  --filter <NAME>       Video filter: sharp, smooth, scanlines, crt
                        (aliases: nearest → sharp)
  --volume <0.0-1.0>    Master volume
  --fullscreen          Start in fullscreen
  --headless            Run without a window (needs a ROM; CI smoke test)
  -h, --help            Show this help
  -V, --version         Show version"
    );
}

pub fn print_version() {
    println!("rs-nes {}", env!("CARGO_PKG_VERSION"));
}

fn next_value<I, S>(iter: &mut std::iter::Peekable<I>, flag: &str) -> Result<String, String>
where
    I: Iterator<Item = S>,
    S: AsRef<str>,
{
    match iter.next() {
        Some(v) if !v.as_ref().starts_with('-') => Ok(v.as_ref().to_string()),
        _ => Err(format!("missing value for {flag}")),
    }
}

fn parse_filter(name: &str) -> Result<VideoFilter, String> {
    match name.trim().to_ascii_lowercase().as_str() {
        "sharp" | "nearest" => Ok(VideoFilter::Sharp),
        "smooth" | "linear" => Ok(VideoFilter::Smooth),
        "scanlines" | "scanline" => Ok(VideoFilter::Scanlines),
        "crt" => Ok(VideoFilter::Crt),
        other => Err(format!(
            "unknown filter `{other}` (expected sharp, smooth, scanlines, crt)"
        )),
    }
}

fn parse_volume(s: &str) -> Result<f32, String> {
    let v: f32 = s
        .trim()
        .parse()
        .map_err(|_| format!("invalid volume `{s}` (expected 0.0–1.0)"))?;
    if !(0.0..=1.0).contains(&v) {
        return Err(format!("volume out of range: {v} (expected 0.0–1.0)"));
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_flags_and_path() {
        let out = parse([
            "--skip-boot",
            "--fullscreen",
            "--filter",
            "crt",
            "--volume=0.5",
            "--roms",
            "/games",
            "mario.nes",
        ])
        .unwrap();
        let ParseOutcome::Run(cli) = out else {
            panic!("expected Run");
        };
        assert!(cli.skip_boot);
        assert!(cli.fullscreen);
        assert_eq!(cli.filter, Some(VideoFilter::Crt));
        assert_eq!(cli.volume, Some(0.5));
        assert_eq!(cli.roms_dirs, vec![PathBuf::from("/games")]);
        assert_eq!(cli.path.as_deref(), Some(std::path::Path::new("mario.nes")));
    }

    #[test]
    fn nearest_alias_is_sharp() {
        let ParseOutcome::Run(cli) = parse(["--filter=nearest"]).unwrap() else {
            panic!("expected Run");
        };
        assert_eq!(cli.filter, Some(VideoFilter::Sharp));
    }

    #[test]
    fn help_and_version() {
        assert!(matches!(parse(["--help"]).unwrap(), ParseOutcome::Help));
        assert!(matches!(parse(["-V"]).unwrap(), ParseOutcome::Version));
    }

    #[test]
    fn rejects_bad_volume() {
        assert!(parse(["--volume", "2"]).is_err());
    }
}
