//! Cover art + game metadata: Libretro thumbnails/DATs, `gamelist.txt`, background scan.

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;
use std::time::Duration;

use image::ImageReader;

use super::gamelist::{self, GameMeta};
use super::library::RomEntry;

const LIBRETRO_NES: &str = "Nintendo - Nintendo Entertainment System";
const LIBRETRO_ROOT: &str = "https://thumbnails.libretro.com";
const LIBRETRO_DB: &str =
    "https://raw.githubusercontent.com/libretro/libretro-database/master/metadat";
const USER_AGENT: &str = "rs-nes/0.1 (NES emulator; cover scan)";

/// Decoded RGBA cover ready for GL upload.
#[derive(Clone)]
pub struct CoverRgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanStatus {
    Idle,
    Running { done: usize, total: usize },
    Finished {
        fetched: usize,
        skipped: usize,
        failed: usize,
    },
}

impl ScanStatus {
    pub fn label(self) -> String {
        match self {
            ScanStatus::Idle => "Scan library".into(),
            ScanStatus::Running { done, total } => {
                format!("Scanning... {done}/{total}")
            }
            ScanStatus::Finished {
                fetched,
                skipped,
                failed,
            } => {
                if failed == 0 {
                    format!("Scan done  +{fetched}  skip {skipped}")
                } else {
                    format!("Scan done  +{fetched}  miss {failed}")
                }
            }
        }
    }

    pub fn is_running(self) -> bool {
        matches!(self, ScanStatus::Running { .. })
    }
}

enum WorkerMsg {
    Progress {
        done: usize,
        total: usize,
        path: Option<PathBuf>,
    },
    Finished {
        fetched: usize,
        skipped: usize,
        failed: usize,
    },
}

/// Background cover scanner + local cover cache helpers.
pub struct CoverService {
    status: ScanStatus,
    rx: Option<Receiver<WorkerMsg>>,
    /// Bump when a new cover file appears so the UI reloads textures.
    pub generation: u64,
}

impl CoverService {
    pub fn new() -> Self {
        Self {
            status: ScanStatus::Idle,
            rx: None,
            generation: 0,
        }
    }

    pub fn status(&self) -> ScanStatus {
        self.status
    }

    /// Poll worker channel; call once per frame from the UI thread.
    pub fn poll(&mut self) {
        let Some(rx) = self.rx.as_ref() else {
            return;
        };
        loop {
            match rx.try_recv() {
                Ok(WorkerMsg::Progress { done, total, path }) => {
                    self.status = ScanStatus::Running { done, total };
                    if path.is_some() {
                        self.generation = self.generation.wrapping_add(1);
                    }
                }
                Ok(WorkerMsg::Finished {
                    fetched,
                    skipped,
                    failed,
                }) => {
                    self.status = ScanStatus::Finished {
                        fetched,
                        skipped,
                        failed,
                    };
                    self.rx = None;
                    // Always bump: gamelist.txt may have new titles/devs/years
                    // even when every cover was already on disk.
                    self.generation = self.generation.wrapping_add(1);
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.rx = None;
                    if !matches!(self.status, ScanStatus::Finished { .. }) {
                        self.status = ScanStatus::Idle;
                    }
                    break;
                }
            }
        }
    }

    /// Start a background job that fetches missing covers for `roms`.
    /// Returns `false` if a scan is already running.
    pub fn start_scan(&mut self, roms: &[RomEntry]) -> bool {
        if self.status.is_running() {
            return false;
        }
        let jobs: Vec<(PathBuf, String)> = roms
            .iter()
            .map(|r| (r.path.clone(), r.name.clone()))
            .collect();
        let total = jobs.len();
        if total == 0 {
            self.status = ScanStatus::Finished {
                fetched: 0,
                skipped: 0,
                failed: 0,
            };
            return true;
        }

        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        self.status = ScanStatus::Running { done: 0, total };

        thread::Builder::new()
            .name("cover-scan".into())
            .spawn(move || run_scan(jobs, tx))
            .expect("spawn cover-scan thread");
        true
    }
}

/// `{rom_dir}/images/{stem}.png` (also accepts existing `.jpg` / `.jpeg`).
pub fn cover_path_for(rom: &Path) -> PathBuf {
    let stem = rom
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("cover");
    let parent = rom.parent().unwrap_or_else(|| Path::new("."));
    parent.join("images").join(format!("{stem}.png"))
}

/// First existing cover file next to the ROM (png/jpg), if any.
pub fn find_existing_cover(rom: &Path) -> Option<PathBuf> {
    let stem = rom.file_stem()?.to_str()?;
    let dir = rom.parent()?.join("images");
    for ext in ["png", "jpg", "jpeg", "PNG", "JPG", "JPEG"] {
        let p = dir.join(format!("{stem}.{ext}"));
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

pub fn load_cover_rgba(path: &Path) -> Option<CoverRgba> {
    let reader = ImageReader::open(path).ok()?.with_guessed_format().ok()?;
    let img = reader.decode().ok()?.into_rgba8();
    let (width, height) = img.dimensions();
    Some(CoverRgba {
        width,
        height,
        pixels: img.into_raw(),
    })
}

fn run_scan(jobs: Vec<(PathBuf, String)>, tx: Sender<WorkerMsg>) {
    let total = jobs.len();
    let listing = fetch_boxart_listing().unwrap_or_default();
    let developers = fetch_dat_field("developer", "developer").unwrap_or_default();
    let years = fetch_dat_field("releaseyear", "releaseyear").unwrap_or_default();
    let mut fetched = 0usize;
    let mut skipped = 0usize;
    let mut failed = 0usize;
    // Per ROM-folder gamelist entries (keyed by cover_rel).
    let mut by_dir: HashMap<PathBuf, HashMap<String, GameMeta>> = HashMap::new();

    for (i, (path, name)) in jobs.into_iter().enumerate() {
        let done = i + 1;
        let mut downloaded: Option<PathBuf> = None;
        let had_cover = find_existing_cover(&path).is_some();

        let matched_remote = match_listing(&name, &listing);
        let canonical = matched_remote
            .as_deref()
            .map(strip_image_ext)
            .unwrap_or_else(|| name.clone());
        let title = official_title(&canonical);
        let developer = lookup_meta(&canonical, &developers).unwrap_or_else(|| "-".into());
        let year = lookup_meta(&canonical, &years).unwrap_or_else(|| "-".into());
        let cover_rel = gamelist::cover_rel_for(&path).unwrap_or_else(|| {
            format!(
                "images/{}.png",
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("cover")
            )
        });

        if had_cover {
            skipped += 1;
        } else {
            match ensure_cover(&path, &name, &listing) {
                Ok(Some(p)) => {
                    fetched += 1;
                    downloaded = Some(p);
                }
                Ok(None) => failed += 1,
                Err(e) => {
                    eprintln!("cover scan {}: {e}", path.display());
                    failed += 1;
                }
            }
        }

        if let Some(dir) = path.parent() {
            by_dir
                .entry(dir.to_path_buf())
                .or_default()
                .insert(
                    cover_rel.clone(),
                    GameMeta {
                        title,
                        developer,
                        year,
                        cover_rel,
                    },
                );
        }

        let _ = tx.send(WorkerMsg::Progress {
            done,
            total,
            path: downloaded,
        });
        // Be polite to the CDN.
        thread::sleep(Duration::from_millis(80));
    }

    for (dir, map) in by_dir {
        let mut entries: Vec<GameMeta> = map.into_values().collect();
        entries.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        // Merge with any previous entries for ROMs not in this scan.
        let mut merged = HashMap::new();
        for e in gamelist::load(&dir) {
            merged.insert(e.cover_rel.clone(), e);
        }
        for e in entries {
            merged.insert(e.cover_rel.clone(), e);
        }
        let mut final_list: Vec<GameMeta> = merged.into_values().collect();
        final_list.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        if let Err(e) = gamelist::save(&dir, &final_list) {
            eprintln!("gamelist {}: {e}", dir.display());
        }
    }

    let _ = tx.send(WorkerMsg::Finished {
        fetched,
        skipped,
        failed,
    });
}

fn strip_image_ext(name: &str) -> String {
    name.rsplit_once('.')
        .filter(|(_, ext)| {
            let e = ext.to_ascii_lowercase();
            matches!(e.as_str(), "png" | "jpg" | "jpeg")
        })
        .map(|(s, _)| s.to_string())
        .unwrap_or_else(|| name.to_string())
}

/// Human title: strip dump tags, fix trailing ", The".
fn official_title(db_name: &str) -> String {
    let mut s = strip_dump_tags(db_name);
    if let Some((base, art)) = s.rsplit_once(", ") {
        let art_l = art.to_ascii_lowercase();
        if matches!(art_l.as_str(), "the" | "a" | "an") {
            s = format!("{art} {base}");
        }
    }
    s
}

fn lookup_meta(db_name: &str, map: &HashMap<String, String>) -> Option<String> {
    for c in name_candidates(db_name) {
        let key = title_key(&c);
        if let Some(v) = map.get(&key) {
            if !v.is_empty() {
                return Some(v.clone());
            }
        }
    }
    let key = title_key(db_name);
    map.get(&key).cloned().filter(|v| !v.is_empty())
}

/// Download a Libretro metadat DAT and index `field` values by [`title_key`].
fn fetch_dat_field(subdir: &str, field: &str) -> Option<HashMap<String, String>> {
    let url = format!(
        "{}/{}/{}",
        LIBRETRO_DB,
        subdir,
        encode_path_segment("Nintendo - Nintendo Entertainment System.dat")
    );
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(60))
        .user_agent(USER_AGENT)
        .build();
    let resp = agent.get(&url).call().ok()?;
    let body = resp.into_string().ok()?;
    Some(parse_dat_field(&body, field))
}

fn parse_dat_field(dat: &str, field: &str) -> HashMap<String, String> {
    let mut out: HashMap<String, (i32, String)> = HashMap::new();
    let mut comment: Option<String> = None;
    let mut value: Option<String> = None;
    let field_prefix = format!("{field} \"");

    for raw in dat.lines() {
        let line = raw.trim();
        if line.starts_with("game (") {
            comment = None;
            value = None;
            continue;
        }
        if line.starts_with("comment \"") {
            if let Some(rest) = line.strip_prefix("comment \"") {
                comment = Some(dat_quoted_value(rest));
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix(&field_prefix) {
            value = Some(dat_quoted_value(rest));
            continue;
        }
        if line == ")" {
            if let (Some(c), Some(v)) = (comment.take(), value.take()) {
                let key = title_key(&c);
                if key.len() < 2 {
                    continue;
                }
                let score = cover_score(&format!("{c}.png"));
                match out.get(&key) {
                    Some((s, _)) if *s >= score => {}
                    _ => {
                        out.insert(key, (score, v));
                    }
                }
            }
        }
    }

    out.into_iter().map(|(k, (_, v))| (k, v)).collect()
}

/// Value inside a DAT `field "...."` line (may end with `"` or `")`).
fn dat_quoted_value(rest: &str) -> String {
    let s = rest.trim();
    if let Some(i) = s.find('"') {
        s[..i].to_string()
    } else {
        s.trim_end_matches(')').trim().to_string()
    }
}

/// Download cover if missing. Returns `Ok(Some(path))` when newly saved,
/// `Ok(None)` when no match, `Err` on I/O / network hard failures after tries.
fn ensure_cover(
    rom: &Path,
    display_name: &str,
    listing: &[String],
) -> Result<Option<PathBuf>, String> {
    if let Some(existing) = find_existing_cover(rom) {
        return Ok(Some(existing));
    }

    let dest = cover_path_for(rom);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("create {}: {e}", parent.display()))?;
    }

    // Prefer a title-key match against the Libretro directory listing (handles
    // No-Intro + TOSEC names, "The Legend…" ↔ "Legend…, The", etc.).
    if let Some(remote) = match_listing(display_name, listing) {
        if let Some(bytes) = download_filename(&remote) {
            fs::write(&dest, &bytes).map_err(|e| format!("write {}: {e}", dest.display()))?;
            return Ok(Some(dest));
        }
    }

    // Fallback: direct URL probes from the ROM stem / common region tags.
    for name in name_candidates(display_name) {
        if let Some(bytes) = download_named_boxart(&name) {
            fs::write(&dest, &bytes).map_err(|e| format!("write {}: {e}", dest.display()))?;
            return Ok(Some(dest));
        }
    }

    Ok(None)
}

fn name_candidates(stem: &str) -> Vec<String> {
    let mut out = Vec::new();
    let push = |v: &mut Vec<String>, s: String| {
        let t = s.trim().to_string();
        if !t.is_empty() && !v.iter().any(|x| x.eq_ignore_ascii_case(&t)) {
            v.push(t);
        }
    };

    push(&mut out, stem.to_string());
    push(&mut out, stem.replace('_', " "));
    push(&mut out, stem.replace('&', "_"));
    push(&mut out, stem.replace(" & ", " _ ").replace('&', "_"));

    let stripped = strip_dump_tags(stem);
    push(&mut out, stripped.clone());
    push(&mut out, stripped.replace('_', " "));

    // Common region suffixes when the ROM stem has no region tag.
    if !stem.contains('(') {
        for region in ["USA", "Europe", "Japan", "World"] {
            push(&mut out, format!("{stripped} ({region})"));
        }
    }

    out
}

/// Remove trailing `(…)` / `[…]` dump tags (No-Intro / GoodTools style).
fn strip_dump_tags(name: &str) -> String {
    let mut s = name.trim().to_string();
    loop {
        let trimmed = s.trim_end();
        let bytes = trimmed.as_bytes();
        if bytes.is_empty() {
            break;
        }
        let end = bytes[bytes.len() - 1];
        let (open, close) = match end {
            b')' => ('(', ')'),
            b']' => ('[', ']'),
            _ => break,
        };
        if let Some(i) = trimmed.rfind(open) {
            // Only strip if the closing bracket is the last char.
            if trimmed[i..].ends_with(close) {
                s = trimmed[..i].trim_end().to_string();
                continue;
            }
        }
        break;
    }
    s
}

fn encode_path_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push_str("%20"),
            _ => {
                out.push('%');
                out.push(char::from(b"0123456789ABCDEF"[(b >> 4) as usize]));
                out.push(char::from(b"0123456789ABCDEF"[(b & 0xf) as usize]));
            }
        }
    }
    out
}

fn boxart_url(filename: &str) -> String {
    format!(
        "{}/{}/Named_Boxarts/{}",
        LIBRETRO_ROOT,
        encode_path_segment(LIBRETRO_NES),
        encode_path_segment(filename)
    )
}

fn download_named_boxart(name: &str) -> Option<Vec<u8>> {
    let filename = if name.ends_with(".png") {
        name.to_string()
    } else {
        format!("{name}.png")
    };
    download_filename(&filename)
}

fn download_filename(filename: &str) -> Option<Vec<u8>> {
    let url = boxart_url(filename);
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(8))
        .timeout_read(Duration::from_secs(20))
        .user_agent(USER_AGENT)
        .build();
    let resp = agent.get(&url).call().ok()?;
    if !(200..300).contains(&resp.status()) {
        return None;
    }
    let mut buf = Vec::new();
    resp.into_reader()
        .take(8 * 1024 * 1024)
        .read_to_end(&mut buf)
        .ok()?;
    // Basic sanity: PNG/JPEG magic.
    if buf.starts_with(&[0x89, b'P', b'N', b'G']) || buf.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(buf)
    } else if buf.len() > 1024 {
        // Some CDNs may still serve valid images; accept mid-size bodies.
        Some(buf)
    } else {
        None
    }
}

fn fetch_boxart_listing() -> Option<Vec<String>> {
    let url = format!(
        "{}/{}/Named_Boxarts/?F=2",
        LIBRETRO_ROOT,
        encode_path_segment(LIBRETRO_NES)
    );
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(60))
        .user_agent(USER_AGENT)
        .build();
    let resp = agent.get(&url).call().ok()?;
    let body = resp.into_string().ok()?;
    Some(parse_apache_listing(&body))
}

fn parse_apache_listing(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    for part in html.split("href=\"") {
        let Some(end) = part.find('"') else {
            continue;
        };
        let href = &part[..end];
        if href.starts_with('?') || href.starts_with('/') || href.starts_with('#') || href.contains("..")
        {
            continue;
        }
        let decoded = percent_decode(href);
        let lower = decoded.to_ascii_lowercase();
        if lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
            out.push(decoded);
        }
    }
    out
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (from_hex(bytes[i + 1]), from_hex(bytes[i + 2])) {
                out.push((h << 4) | l);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            out.push(b' ');
        } else {
            out.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn from_hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn match_listing(stem: &str, listing: &[String]) -> Option<String> {
    if listing.is_empty() {
        return None;
    }

    // Exact filename hits first (ROM stem already matches Libretro naming).
    for c in name_candidates(stem) {
        let want = if c.to_ascii_lowercase().ends_with(".png") {
            c
        } else {
            format!("{c}.png")
        };
        if let Some(hit) = listing.iter().find(|f| f.eq_ignore_ascii_case(&want)) {
            return Some(hit.clone());
        }
    }

    let want_key = title_key(stem);
    if want_key.len() < 3 {
        return None;
    }

    let mut best: Option<(i32, &String)> = None;
    for f in listing {
        if title_key(f) != want_key {
            continue;
        }
        let score = cover_score(f);
        match best {
            Some((s, _)) if s >= score => {}
            _ => best = Some((score, f)),
        }
    }
    best.map(|(_, f)| f.clone())
}

/// Comparable game title: strip dump tags / regions, normalize articles & punctuation.
fn title_key(name: &str) -> String {
    let stem = name
        .rsplit_once('.')
        .filter(|(_, ext)| {
            let e = ext.to_ascii_lowercase();
            matches!(e.as_str(), "png" | "jpg" | "jpeg" | "nes")
        })
        .map(|(s, _)| s)
        .unwrap_or(name);

    // Drop trailing GoodTools-style `[…]` tags only (keep `(region)` for split below).
    let mut s = stem.to_string();
    loop {
        let t = s.trim_end();
        if t.ends_with(']') {
            if let Some(i) = t.rfind('[') {
                s = t[..i].trim_end().to_string();
                continue;
            }
        }
        break;
    }

    // Title is everything before the first region/year parenthesis group.
    let head = s.split(" (").next().unwrap_or(&s).trim();

    // "Legend of Zelda, The" → "The Legend of Zelda"
    let head = if let Some((base, art)) = head.rsplit_once(", ") {
        let art_l = art.to_ascii_lowercase();
        if matches!(art_l.as_str(), "the" | "a" | "an") {
            format!("{art} {base}")
        } else {
            head.to_string()
        }
    } else {
        head.to_string()
    };

    let cleaned: String = head
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == ' ' {
                c.to_ascii_lowercase()
            } else if c == '_' || c == '-' {
                ' '
            } else {
                ' '
            }
        })
        .collect();
    let mut words: Vec<&str> = cleaned.split_whitespace().collect();
    if let Some(first) = words.first() {
        if matches!(*first, "the" | "a" | "an") {
            words.remove(0);
        }
    }
    words.join(" ")
}

fn cover_score(filename: &str) -> i32 {
    let low = filename.to_ascii_lowercase();
    let mut score = 10;
    // Prefer clean retail dumps over hacks / translations / bad dumps.
    if low.contains("[h")
        || low.contains("[b")
        || low.contains("[tr ")
        || low.contains("[p]")
        || low.contains("[t]")
        || low.contains("[o]")
    {
        score -= 8;
    }
    if low.contains("(usa)") {
        score += 6;
    } else if low.contains("(world)") {
        score += 5;
    } else if low.contains("(us)") || low.contains("(jp-us)") || low.contains("(europe)") {
        score += 3;
    }
    if !low.contains('[') {
        score += 3;
    }
    // Slight preference for shorter, simpler names.
    score -= (filename.len() as i32 / 40).min(4);
    score
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_tags_no_intro() {
        assert_eq!(
            strip_dump_tags("Super Mario Bros. (USA)"),
            "Super Mario Bros."
        );
        assert_eq!(strip_dump_tags("Zelda [!]"), "Zelda");
    }

    #[test]
    fn candidates_include_regions() {
        let c = name_candidates("Metroid");
        assert!(c.iter().any(|x| x == "Metroid (USA)"));
    }

    #[test]
    fn title_key_articles_and_regions() {
        assert_eq!(
            title_key("The Legend of Zelda (USA)"),
            title_key("Legend of Zelda, The (USA).png")
        );
        assert_eq!(
            title_key("Super Mario Bros. (USA)"),
            title_key("Super Mario Bros. (World).png")
        );
        assert_ne!(title_key("Mega Man (USA)"), title_key("Mega Man 6 (USA).png"));
    }

    #[test]
    fn match_prefers_usa_clean() {
        let listing = vec![
            "Metroid (Europe).png".into(),
            "Metroid (USA).png".into(),
            "Metroid (USA)[h].png".into(),
        ];
        assert_eq!(
            match_listing("Metroid (USA)", &listing).as_deref(),
            Some("Metroid (USA).png")
        );
    }

    #[test]
    fn official_title_fixes_article() {
        assert_eq!(
            official_title("Legend of Zelda, The (USA)"),
            "The Legend of Zelda"
        );
        assert_eq!(official_title("Metroid (USA)"), "Metroid");
    }

    #[test]
    fn parse_developer_dat() {
        let dat = r#"
game (
	comment "Metroid (USA)"
	developer "Nintendo"
	rom ( crc AABBCCDD )
)
game (
	comment "Legend of Zelda, The (USA)"
	developer "Nintendo"
	rom ( crc 11223344 )
)
"#;
        let map = parse_dat_field(dat, "developer");
        assert_eq!(
            map.get(&title_key("Metroid (USA)")).map(String::as_str),
            Some("Nintendo")
        );
        assert_eq!(
            map.get(&title_key("Legend of Zelda, The (USA)"))
                .map(String::as_str),
            Some("Nintendo")
        );
        assert_eq!(
            title_key("Legend of Zelda, The (USA)"),
            title_key("The Legend of Zelda")
        );
    }
}

#[cfg(test)]
mod net_tests {
    use super::*;

    #[test]
    #[ignore = "network"]
    fn download_metroid_cover() {
        let dir = std::env::temp_dir().join("rs-nes-cover-net");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let rom = dir.join("Metroid (USA).nes");
        std::fs::write(&rom, b"NES\x1a").unwrap();
        let listing = fetch_boxart_listing().expect("listing");
        let path = ensure_cover(&rom, "Metroid (USA)", &listing)
            .unwrap()
            .expect("cover found");
        assert!(path.is_file());
        assert!(path.metadata().unwrap().len() > 1000);
        let img = load_cover_rgba(&path).expect("decode");
        assert!(img.width > 10 && img.height > 10);
    }
}
