pub mod edit;
pub mod layout;
pub mod widget;
pub mod xpm;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use image::GenericImageView;
pub use layout::SkinPixmapInfo;
use xpm::XpmImage;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkinEntry {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SkinPixmapKind {
    Main,
    CButtons,
    Titlebar,
    ShufRep,
    Text,
    Volume,
    Balance,
    MonoStereo,
    PlayPause,
    Numbers,
    PosBar,
    PlEdit,
    EqMain,
    EqEx,
}

impl SkinPixmapKind {
    pub const ALL: [SkinPixmapKind; 14] = [
        SkinPixmapKind::Main,
        SkinPixmapKind::CButtons,
        SkinPixmapKind::Titlebar,
        SkinPixmapKind::ShufRep,
        SkinPixmapKind::Text,
        SkinPixmapKind::Volume,
        SkinPixmapKind::Balance,
        SkinPixmapKind::MonoStereo,
        SkinPixmapKind::PlayPause,
        SkinPixmapKind::Numbers,
        SkinPixmapKind::PosBar,
        SkinPixmapKind::PlEdit,
        SkinPixmapKind::EqMain,
        SkinPixmapKind::EqEx,
    ];

    pub fn info(self) -> SkinPixmapInfo {
        layout::pixmap_info(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefaultSkin {
    pixmaps: BTreeMap<SkinPixmapKind, XpmImage>,
    vis_colors: [[u8; 3]; 24],
    playlist_colors: PlaylistColors,
    region_masks: RegionMasks,
    text_colors: TextColors,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaylistColors {
    pub normal: [u8; 3],
    pub current: [u8; 3],
    pub normal_bg: [u8; 3],
    pub selected_bg: [u8; 3],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextColors {
    pub background: [[u8; 3]; 6],
    pub foreground: [[u8; 3]; 6],
}

impl Default for TextColors {
    fn default() -> Self {
        Self {
            background: [[0, 0, 0]; 6],
            foreground: [[255, 255, 255]; 6],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RegionMasks {
    pub normal: Option<SkinMask>,
    pub window_shade: Option<SkinMask>,
    pub equalizer: Option<SkinMask>,
    pub equalizer_ws: Option<SkinMask>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkinMask {
    polygons: Vec<Vec<[i32; 2]>>,
}

impl SkinMask {
    pub fn polygons(&self) -> &[Vec<[i32; 2]>] {
        &self.polygons
    }

    pub fn scaled_polygons(&self, doublesize: bool) -> Vec<Vec<[i32; 2]>> {
        let scale = if doublesize { 2 } else { 1 };
        self.polygons
            .iter()
            .map(|polygon| {
                polygon
                    .iter()
                    .map(|point| [point[0] * scale, point[1] * scale])
                    .collect()
            })
            .collect()
    }
}

pub const DEFAULT_PLAYLIST_COLORS: PlaylistColors = PlaylistColors {
    normal: [0, 255, 0],
    current: [255, 255, 255],
    normal_bg: [0, 0, 0],
    selected_bg: [0, 0, 102],
};

pub const DEFAULT_VIS_COLORS: [[u8; 3]; 24] = [
    [9, 34, 53],
    [10, 18, 26],
    [0, 54, 108],
    [0, 58, 116],
    [0, 62, 124],
    [0, 66, 132],
    [0, 70, 140],
    [0, 74, 148],
    [0, 78, 156],
    [0, 82, 164],
    [0, 86, 172],
    [0, 92, 184],
    [0, 98, 196],
    [0, 104, 208],
    [0, 110, 220],
    [0, 116, 232],
    [0, 122, 244],
    [0, 128, 255],
    [0, 128, 255],
    [0, 104, 208],
    [0, 80, 160],
    [0, 56, 112],
    [0, 32, 64],
    [200, 200, 200],
];

impl DefaultSkin {
    pub fn load_bundled() -> io::Result<Self> {
        const BUNDLED_XPMS: &[(SkinPixmapKind, &str)] = &[
            (
                SkinPixmapKind::Main,
                include_str!("../../data/defskin/main.xpm"),
            ),
            (
                SkinPixmapKind::CButtons,
                include_str!("../../data/defskin/cbuttons.xpm"),
            ),
            (
                SkinPixmapKind::Titlebar,
                include_str!("../../data/defskin/titlebar.xpm"),
            ),
            (
                SkinPixmapKind::ShufRep,
                include_str!("../../data/defskin/shufrep.xpm"),
            ),
            (
                SkinPixmapKind::Text,
                include_str!("../../data/defskin/text.xpm"),
            ),
            (
                SkinPixmapKind::Volume,
                include_str!("../../data/defskin/volume.xpm"),
            ),
            (
                SkinPixmapKind::MonoStereo,
                include_str!("../../data/defskin/monoster.xpm"),
            ),
            (
                SkinPixmapKind::PlayPause,
                include_str!("../../data/defskin/playpaus.xpm"),
            ),
            (
                SkinPixmapKind::Numbers,
                include_str!("../../data/defskin/nums_ex.xpm"),
            ),
            (
                SkinPixmapKind::PosBar,
                include_str!("../../data/defskin/posbar.xpm"),
            ),
            (
                SkinPixmapKind::PlEdit,
                include_str!("../../data/defskin/pledit.xpm"),
            ),
            (
                SkinPixmapKind::EqMain,
                include_str!("../../data/defskin/eqmain.xpm"),
            ),
            (
                SkinPixmapKind::EqEx,
                include_str!("../../data/defskin/eq_ex.xpm"),
            ),
        ];

        let mut pixmaps = BTreeMap::new();
        for (kind, contents) in BUNDLED_XPMS {
            let image = XpmImage::parse(contents).map_err(|err| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("bundled {}.xpm: {err}", kind.info().file_stem),
                )
            })?;
            pixmaps.insert(*kind, image);
        }
        apply_balance_fallback(&mut pixmaps);
        let text_colors = text_colors_from_pixmaps(&pixmaps);
        Ok(Self {
            pixmaps,
            vis_colors: DEFAULT_VIS_COLORS,
            playlist_colors: DEFAULT_PLAYLIST_COLORS,
            region_masks: RegionMasks::default(),
            text_colors,
        })
    }

    pub fn load_from_dir(dir: &Path) -> io::Result<Self> {
        let mut skin = Self::load_bundled()?;
        let mut loaded = BTreeSet::new();

        for kind in SkinPixmapKind::ALL {
            let mut numbers_fallback = false;
            let mut path = Self::find_skin_file(dir, kind.info().file_stem);
            if path.is_none() && kind == SkinPixmapKind::Numbers {
                path = Self::find_skin_file(dir, "numbers");
                numbers_fallback = path.is_some();
            }
            let Some(path) = path else {
                continue;
            };

            let mut image = Self::load_skin_image(&path)?;
            if numbers_fallback {
                image = expand_numbers_fallback(image);
            }
            skin.pixmaps.insert(kind, image);
            loaded.insert(kind);
        }

        apply_loaded_balance_fallback(&mut skin.pixmaps, &loaded);
        apply_loaded_eq_ex_fallback(&mut skin.pixmaps, &loaded);
        skin.vis_colors = load_vis_colors_from_dir(dir)?;
        skin.playlist_colors = load_playlist_colors_from_dir(dir)?;
        skin.region_masks = load_region_masks_from_dir(dir)?;
        skin.text_colors = text_colors_from_pixmaps(&skin.pixmaps);

        Ok(skin)
    }

    pub fn load_from_path(path: &Path) -> io::Result<Self> {
        if path.is_dir() {
            Self::load_from_dir(path)
        } else {
            Self::load_from_archive(path)
        }
    }

    fn load_from_archive(path: &Path) -> io::Result<Self> {
        let entries = archive_entries(path)?;
        let mut skin = Self::load_bundled()?;
        let mut loaded = BTreeSet::new();

        for kind in SkinPixmapKind::ALL {
            let mut numbers_fallback = false;
            let mut entry = find_archive_skin_entry(&entries, kind.info().file_stem);
            if entry.is_none() && kind == SkinPixmapKind::Numbers {
                entry = find_archive_skin_entry(&entries, "numbers");
                numbers_fallback = entry.is_some();
            }
            let Some((name, contents)) = entry else {
                continue;
            };

            let mut image = Self::load_skin_image_bytes(
                &format!("{}:{name}", path.display()),
                Path::new(name),
                contents,
            )?;
            if numbers_fallback {
                image = expand_numbers_fallback(image);
            }
            skin.pixmaps.insert(kind, image);
            loaded.insert(kind);
        }

        apply_loaded_balance_fallback(&mut skin.pixmaps, &loaded);
        apply_loaded_eq_ex_fallback(&mut skin.pixmaps, &loaded);
        skin.vis_colors = load_vis_colors_from_archive(&entries)?;
        skin.playlist_colors = load_playlist_colors_from_archive(&entries)?;
        skin.region_masks = load_region_masks_from_archive(&entries)?;
        skin.text_colors = text_colors_from_pixmaps(&skin.pixmaps);

        Ok(skin)
    }

    fn find_skin_file(dir: &Path, name: &str) -> Option<PathBuf> {
        let lower = name.to_ascii_lowercase();
        let upper = name.to_ascii_uppercase();
        let mut title = lower.clone();
        if let Some(first) = title.get_mut(0..1) {
            first.make_ascii_uppercase();
        }
        let cases = [name, lower.as_str(), upper.as_str(), title.as_str()];
        let exts = [".bmp", ".BMP", ".png", ".PNG", ".xpm", ".XPM"];
        let mut candidates = Vec::new();

        for ext in exts {
            for case in cases {
                candidates.push(format!("{case}{ext}"));
            }
        }

        for candidate in &candidates {
            if let Some(path) = find_file_in_dir_case_insensitive(dir, candidate) {
                return Some(path);
            }
        }
        for candidate in &candidates {
            if let Some(path) = find_file_recursively_case_insensitive(dir, candidate) {
                return Some(path);
            }
        }
        None
    }

    fn load_skin_image(path: &Path) -> io::Result<XpmImage> {
        let contents = fs::read(path)?;
        Self::load_skin_image_bytes(&path.display().to_string(), path, &contents)
    }

    fn load_skin_image_bytes(label: &str, path: &Path, contents: &[u8]) -> io::Result<XpmImage> {
        if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("xpm"))
        {
            let contents = std::str::from_utf8(contents).map_err(|err| {
                io::Error::new(io::ErrorKind::InvalidData, format!("{label}: {err}"))
            });
            return contents.and_then(|contents| {
                XpmImage::parse(contents).map_err(|err| {
                    io::Error::new(io::ErrorKind::InvalidData, format!("{label}: {err}"))
                })
            });
        }

        let decoded = image::load_from_memory(contents)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, format!("{label}: {err}")))?;
        let (width, height) = decoded.dimensions();
        let rgba = decoded.to_rgba8();
        let mut argb = Vec::with_capacity((width as usize) * (height as usize));

        for pixel in rgba.pixels() {
            let [r, g, b, mut a] = pixel.0;
            if r == 48 && g == 255 && b == 50 {
                a = 0;
            }
            let pr = ((u16::from(r) * u16::from(a) + 127) / 255) as u32;
            let pg = ((u16::from(g) * u16::from(a) + 127) / 255) as u32;
            let pb = ((u16::from(b) * u16::from(a) + 127) / 255) as u32;
            argb.push((u32::from(a) << 24) | (pr << 16) | (pg << 8) | pb);
        }

        XpmImage::from_argb_pixels(width as usize, height as usize, argb)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, format!("{label}: {err}")))
    }

    pub fn loaded_pixmap_count(&self) -> usize {
        self.pixmaps.len()
    }

    pub fn get(&self, kind: SkinPixmapKind) -> Option<&XpmImage> {
        self.pixmaps.get(&kind)
    }

    pub fn vis_colors(&self) -> &[[u8; 3]; 24] {
        &self.vis_colors
    }

    pub fn playlist_colors(&self) -> PlaylistColors {
        self.playlist_colors
    }

    pub fn region_masks(&self) -> &RegionMasks {
        &self.region_masks
    }

    pub fn text_colors(&self) -> TextColors {
        self.text_colors
    }
}

pub fn discover_skins_in_dirs<I, P>(dirs: I) -> io::Result<Vec<SkinEntry>>
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    crate::perf_span!("skin_discovery");
    let mut skins = Vec::new();
    for dir in dirs {
        scan_skin_dir(dir.as_ref(), &mut skins)?;
    }
    skins.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
            .then_with(|| a.path.cmp(&b.path))
    });
    Ok(skins)
}

pub fn skin_browser_search_dirs(
    user_config_dir: &Path,
    home_dir: &Path,
    system_skin_dir: &Path,
    skinsdir_env: Option<&str>,
) -> Vec<PathBuf> {
    let mut dirs = vec![
        user_config_dir.join("xmms").join("Skins"),
        home_dir.join(".xmms").join("Skins"),
        system_skin_dir.to_path_buf(),
    ];
    if let Some(skinsdir_env) = skinsdir_env {
        dirs.extend(
            skinsdir_env
                .split(':')
                .filter(|dir| !dir.is_empty())
                .map(PathBuf::from),
        );
    }
    dirs
}

fn scan_skin_dir(dir: &Path, skins: &mut Vec<SkinEntry>) -> io::Result<()> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Ok(());
    };

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if file_name.starts_with('.') {
            continue;
        }

        let file_type = entry.file_type()?;
        if file_type.is_dir() || (file_type.is_file() && is_skin_archive_path(&path)) {
            skins.push(SkinEntry {
                name: skin_display_name(&path),
                path,
            });
        }
    }
    Ok(())
}

fn is_skin_archive_path(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    [
        ".zip", ".wsz", ".tar", ".tar.gz", ".tgz", ".tar.bz2", ".tbz2",
    ]
    .iter()
    .any(|suffix| name.ends_with(suffix))
}

fn skin_display_name(path: &Path) -> String {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return path.display().to_string();
    };

    let mut name = file_name.to_string();
    if is_skin_archive_path(path) {
        if let Some((base, _ext)) = name.rsplit_once('.') {
            name = base.to_string();
        }
        if name.to_ascii_lowercase().strip_suffix(".tar").is_some() {
            name.truncate(name.len() - 4);
        }
    }
    name
}

fn archive_entries(path: &Path) -> io::Result<Vec<(String, Vec<u8>)>> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if name.ends_with(".zip") || name.ends_with(".wsz") {
        return zip_archive_entries(path);
    }

    if name.ends_with(".tar") {
        let file = File::open(path)?;
        return tar_archive_entries(file);
    }

    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        let file = File::open(path)?;
        return tar_archive_entries(flate2::read::GzDecoder::new(file));
    }

    if name.ends_with(".tar.bz2") || name.ends_with(".tbz2") {
        let file = File::open(path)?;
        return tar_archive_entries(bzip2::read::BzDecoder::new(file));
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("unsupported skin archive format: {}", path.display()),
    ))
}

fn zip_archive_entries(path: &Path) -> io::Result<Vec<(String, Vec<u8>)>> {
    let file = File::open(path)?;
    let mut archive = zip::ZipArchive::new(file).map_err(zip_error)?;
    let mut entries = Vec::new();

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(zip_error)?;
        if entry.is_dir() {
            continue;
        }

        let mut contents = Vec::new();
        entry.read_to_end(&mut contents)?;
        entries.push((entry.name().to_string(), contents));
    }

    Ok(entries)
}

fn tar_archive_entries<R: Read>(reader: R) -> io::Result<Vec<(String, Vec<u8>)>> {
    let mut archive = tar::Archive::new(reader);
    let mut entries = Vec::new();

    for entry in archive.entries()? {
        let mut entry = entry?;
        if !entry.header().entry_type().is_file() {
            continue;
        }

        let path = entry.path()?.to_string_lossy().into_owned();
        let mut contents = Vec::new();
        entry.read_to_end(&mut contents)?;
        entries.push((path, contents));
    }

    Ok(entries)
}

fn zip_error(err: zip::result::ZipError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, err)
}

fn find_file_in_dir_case_insensitive(dir: &Path, file: &str) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries {
        let entry = entry.ok()?;
        if !entry.file_type().ok()?.is_file() {
            continue;
        }
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        if file_name.eq_ignore_ascii_case(file) {
            return Some(entry.path());
        }
    }
    None
}

fn find_file_recursively_case_insensitive(dir: &Path, file: &str) -> Option<PathBuf> {
    if let Some(path) = find_file_in_dir_case_insensitive(dir, file) {
        return Some(path);
    }

    let entries = fs::read_dir(dir).ok()?;
    for entry in entries {
        let entry = entry.ok()?;
        if !entry.file_type().ok()?.is_dir() {
            continue;
        }
        if let Some(path) = find_file_recursively_case_insensitive(&entry.path(), file) {
            return Some(path);
        }
    }
    None
}

fn find_archive_skin_entry<'a>(
    entries: &'a [(String, Vec<u8>)],
    name: &str,
) -> Option<(&'a str, &'a [u8])> {
    for extension in ["bmp", "png", "xpm"] {
        for (entry_name, contents) in entries {
            let entry_path = Path::new(entry_name);
            let Some(stem) = entry_path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            let Some(entry_extension) = entry_path.extension().and_then(|ext| ext.to_str()) else {
                continue;
            };
            if stem.eq_ignore_ascii_case(name) && entry_extension.eq_ignore_ascii_case(extension) {
                return Some((entry_name, contents));
            }
        }
    }

    None
}

fn load_vis_colors_from_dir(dir: &Path) -> io::Result<[[u8; 3]; 24]> {
    let Some(path) = find_file_recursively_case_insensitive(dir, "viscolor.txt") else {
        return Ok(DEFAULT_VIS_COLORS);
    };
    let contents = fs::read_to_string(&path)?;
    Ok(parse_vis_colors(&contents))
}

fn load_vis_colors_from_archive(entries: &[(String, Vec<u8>)]) -> io::Result<[[u8; 3]; 24]> {
    let Some((name, contents)) = entries.iter().find(|(name, _)| {
        Path::new(name)
            .file_name()
            .and_then(|file_name| file_name.to_str())
            .is_some_and(|file_name| file_name.eq_ignore_ascii_case("viscolor.txt"))
    }) else {
        return Ok(DEFAULT_VIS_COLORS);
    };

    let contents = std::str::from_utf8(contents)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, format!("{name}: {err}")))?;
    Ok(parse_vis_colors(contents))
}

fn load_playlist_colors_from_dir(dir: &Path) -> io::Result<PlaylistColors> {
    let Some(path) = find_file_recursively_case_insensitive(dir, "pledit.txt") else {
        return Ok(DEFAULT_PLAYLIST_COLORS);
    };
    let contents = fs::read_to_string(&path)?;
    Ok(parse_playlist_colors(&contents))
}

fn load_playlist_colors_from_archive(entries: &[(String, Vec<u8>)]) -> io::Result<PlaylistColors> {
    let Some((name, contents)) = entries.iter().find(|(name, _)| {
        Path::new(name)
            .file_name()
            .and_then(|file_name| file_name.to_str())
            .is_some_and(|file_name| file_name.eq_ignore_ascii_case("pledit.txt"))
    }) else {
        return Ok(DEFAULT_PLAYLIST_COLORS);
    };

    let contents = std::str::from_utf8(contents)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, format!("{name}: {err}")))?;
    Ok(parse_playlist_colors(contents))
}

fn load_region_masks_from_dir(dir: &Path) -> io::Result<RegionMasks> {
    let Some(path) = find_file_recursively_case_insensitive(dir, "region.txt") else {
        return Ok(RegionMasks::default());
    };
    let contents = fs::read_to_string(&path)?;
    Ok(parse_region_masks(&contents))
}

fn load_region_masks_from_archive(entries: &[(String, Vec<u8>)]) -> io::Result<RegionMasks> {
    let Some((name, contents)) = entries.iter().find(|(name, _)| {
        Path::new(name)
            .file_name()
            .and_then(|file_name| file_name.to_str())
            .is_some_and(|file_name| file_name.eq_ignore_ascii_case("region.txt"))
    }) else {
        return Ok(RegionMasks::default());
    };

    let contents = std::str::from_utf8(contents)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, format!("{name}: {err}")))?;
    Ok(parse_region_masks(contents))
}

fn parse_region_masks(contents: &str) -> RegionMasks {
    RegionMasks {
        normal: parse_region_mask_section(contents, "Normal"),
        window_shade: parse_region_mask_section(contents, "WindowShade"),
        equalizer: parse_region_mask_section(contents, "Equalizer"),
        equalizer_ws: parse_region_mask_section(contents, "EqualizerWS"),
    }
}

fn parse_region_mask_section(contents: &str, section: &str) -> Option<SkinMask> {
    let nums = parse_ini_ints(contents, section, "NumPoints")?;
    let points = parse_ini_ints(contents, section, "PointList")?;
    let mut polygons = Vec::new();
    let mut offset = 0;

    for count in nums {
        let count = usize::try_from(count).ok()?;
        let point_values = count.checked_mul(2)?;
        if points.len().saturating_sub(offset) < point_values {
            continue;
        }
        let mut polygon = Vec::with_capacity(count);
        for index in 0..count {
            polygon.push([points[offset + index * 2], points[offset + index * 2 + 1]]);
        }
        offset += point_values;
        polygons.push(polygon);
    }

    (!polygons.is_empty()).then_some(SkinMask { polygons })
}

fn parse_playlist_colors(contents: &str) -> PlaylistColors {
    let mut colors = DEFAULT_PLAYLIST_COLORS;

    for (key, target) in [
        ("normal", &mut colors.normal),
        ("current", &mut colors.current),
        ("normalbg", &mut colors.normal_bg),
        ("selectedbg", &mut colors.selected_bg),
    ] {
        let value = read_ini_value(contents, "text", key)
            .or_else(|| read_top_level_ini_value(contents, key));
        if let Some(value) = value.and_then(|value| parse_skin_color(&value)) {
            *target = value;
        }
    }

    colors
}

fn parse_ini_ints(contents: &str, section: &str, key: &str) -> Option<Vec<i32>> {
    let value = read_ini_value(contents, section, key)?;
    Some(parse_int_list(&value))
}

fn read_ini_value(contents: &str, section: &str, key: &str) -> Option<String> {
    let mut in_section = false;
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(section_name) = line.strip_prefix('[').and_then(|line| line.split_once(']')) {
            in_section = section_name.0.eq_ignore_ascii_case(section);
            continue;
        }
        if !in_section {
            continue;
        }
        let Some((line_key, value)) = line.split_once('=') else {
            continue;
        };
        if line_key.trim().eq_ignore_ascii_case(key) {
            let value = value
                .split_once(';')
                .map_or(value, |(value, _)| value)
                .trim();
            return Some(value.to_string());
        }
    }
    None
}

fn read_top_level_ini_value(contents: &str, key: &str) -> Option<String> {
    for line in contents.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            break;
        }
        let Some((line_key, value)) = line.split_once('=') else {
            continue;
        };
        if line_key.trim().eq_ignore_ascii_case(key) {
            let value = value
                .split_once(';')
                .map_or(value, |(value, _)| value)
                .trim();
            return Some(value.to_string());
        }
    }
    None
}

fn parse_int_list(value: &str) -> Vec<i32> {
    let mut values = Vec::new();
    let mut start = None;
    for (index, ch) in value.char_indices() {
        if start.is_none() && (ch == '-' || ch == '+' || ch.is_ascii_digit()) {
            start = Some(index);
            continue;
        }
        if start.is_some() && !ch.is_ascii_digit() && ch != '-' && ch != '+' {
            if let Some(begin) = start.take() {
                if let Ok(value) = value[begin..index].parse() {
                    values.push(value);
                }
            }
        }
    }
    if let Some(begin) = start {
        if let Ok(value) = value[begin..].parse() {
            values.push(value);
        }
    }
    values
}

fn parse_skin_color(value: &str) -> Option<[u8; 3]> {
    let value = value.trim();
    let value = value.strip_prefix('#').unwrap_or(value);
    if value.len() < 2 {
        return None;
    }
    let mut color = [0, 0, 0];
    if value.len() >= 6 {
        color[0] = parse_hex_byte(&value[0..2])?;
        color[1] = parse_hex_byte(&value[2..4])?;
        color[2] = parse_hex_byte(&value[4..6])?;
    } else if value.len() >= 4 {
        color[1] = parse_hex_byte(&value[0..2])?;
        color[2] = parse_hex_byte(&value[2..4])?;
    } else {
        color[2] = parse_hex_byte(&value[0..2])?;
    }
    Some(color)
}

fn parse_hex_byte(value: &str) -> Option<u8> {
    if !value.as_bytes().iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    u8::from_str_radix(value, 16).ok()
}

fn parse_vis_colors(contents: &str) -> [[u8; 3]; 24] {
    let mut colors = DEFAULT_VIS_COLORS;

    for (index, line) in contents.lines().take(24).enumerate() {
        let values: Vec<_> = if line.contains(',') {
            line.split(',').collect()
        } else {
            line.split_whitespace().collect()
        };
        if values.len() < 3 {
            continue;
        }

        let Some(r) = parse_c_int(values[0].trim()) else {
            continue;
        };
        let Some(g) = parse_c_int(values[1].trim()) else {
            continue;
        };
        let Some(b) = parse_c_int(values[2].trim()) else {
            continue;
        };
        colors[index] = [clamp_u8(r), clamp_u8(g), clamp_u8(b)];
    }

    colors
}

fn parse_c_int(value: &str) -> Option<i32> {
    let mut end = 0;
    for (index, ch) in value.char_indices() {
        if index == 0 && (ch == '-' || ch == '+') {
            end = ch.len_utf8();
            continue;
        }
        if !ch.is_ascii_digit() {
            break;
        }
        end = index + ch.len_utf8();
    }

    if end == 0 || value[..end].chars().all(|ch| ch == '-' || ch == '+') {
        return None;
    }

    value[..end].parse().ok()
}

fn clamp_u8(value: i32) -> u8 {
    value.clamp(0, 255) as u8
}

fn apply_balance_fallback(pixmaps: &mut BTreeMap<SkinPixmapKind, XpmImage>) {
    if !pixmaps.contains_key(&SkinPixmapKind::Balance) {
        if let Some(volume) = pixmaps.get(&SkinPixmapKind::Volume).cloned() {
            pixmaps.insert(SkinPixmapKind::Balance, volume);
        }
    }
}

fn apply_loaded_balance_fallback(
    pixmaps: &mut BTreeMap<SkinPixmapKind, XpmImage>,
    loaded: &BTreeSet<SkinPixmapKind>,
) {
    if !loaded.contains(&SkinPixmapKind::Balance) {
        if let Some(volume) = pixmaps.get(&SkinPixmapKind::Volume).cloned() {
            pixmaps.insert(SkinPixmapKind::Balance, volume);
        }
    }
}

fn apply_loaded_eq_ex_fallback(
    pixmaps: &mut BTreeMap<SkinPixmapKind, XpmImage>,
    loaded: &BTreeSet<SkinPixmapKind>,
) {
    if !loaded.contains(&SkinPixmapKind::EqMain) || loaded.contains(&SkinPixmapKind::EqEx) {
        return;
    }
    let Some(eqmain) = pixmaps.get(&SkinPixmapKind::EqMain) else {
        return;
    };
    let Some(eq_ex) = pixmaps.get(&SkinPixmapKind::EqEx) else {
        return;
    };
    let width = layout::EQUALIZER_WINDOW_WIDTH as usize;
    let title_height = layout::MAIN_TITLEBAR_HEIGHT as usize;
    if eqmain.width() < width
        || eqmain.height() < title_height
        || eq_ex.width() < width
        || eq_ex.height() < title_height * 2 + 1
    {
        return;
    }

    let focused_y = if eqmain.height() >= 163 { 134 } else { 0 };
    let unfocused_y = if eqmain.height() >= 163 {
        149
    } else {
        focused_y
    };
    let mut pixels = eq_ex.pixels_argb().to_vec();
    for (source_y, destination_y) in [(focused_y, 0), (unfocused_y, 15)] {
        for row in 0..title_height {
            let source = (source_y + row) * eqmain.width();
            let destination = (destination_y + row) * eq_ex.width();
            pixels[destination..destination + width]
                .copy_from_slice(&eqmain.pixels_argb()[source..source + width]);
        }
    }
    pixmaps.insert(
        SkinPixmapKind::EqEx,
        XpmImage::from_argb_pixels(eq_ex.width(), eq_ex.height(), pixels)
            .expect("equalizer fallback preserves source dimensions"),
    );
}

fn text_colors_from_pixmaps(pixmaps: &BTreeMap<SkinPixmapKind, XpmImage>) -> TextColors {
    let Some(text) = pixmaps.get(&SkinPixmapKind::Text) else {
        return TextColors::default();
    };
    if text.width() <= 151 || text.height() < 6 {
        return TextColors::default();
    }

    let mut colors = TextColors::default();
    for y in 0..6 {
        let bg = rgb_from_argb(text.pixel_argb(151, y).unwrap_or(0));
        colors.background[y] = bg;

        let bg_luminance = luminance(bg);
        let mut max_distance = 0.0;
        let mut fg = colors.foreground[y];
        for x in 1..150 {
            let candidate = rgb_from_argb(text.pixel_argb(x, y).unwrap_or(0));
            let distance = (luminance(candidate) - bg_luminance).abs();
            if distance > max_distance {
                max_distance = distance;
                fg = candidate;
            }
        }
        colors.foreground[y] = fg;
    }
    colors
}

fn rgb_from_argb(argb: u32) -> [u8; 3] {
    let a = ((argb >> 24) & 0xff) as u8;
    let r = ((argb >> 16) & 0xff) as u8;
    let g = ((argb >> 8) & 0xff) as u8;
    let b = (argb & 0xff) as u8;
    if a == 0 || a == 255 {
        return [r, g, b];
    }
    [
        ((u16::from(r) * 255) / u16::from(a)).min(255) as u8,
        ((u16::from(g) * 255) / u16::from(a)).min(255) as u8,
        ((u16::from(b) * 255) / u16::from(a)).min(255) as u8,
    ]
}

fn luminance(color: [u8; 3]) -> f64 {
    0.212671 * f64::from(color[0]) + 0.715160 * f64::from(color[1]) + 0.072169 * f64::from(color[2])
}

fn expand_numbers_fallback(image: XpmImage) -> XpmImage {
    if image.width() < 99 || image.height() < 13 {
        return image;
    }

    let mut argb = vec![0; 108 * image.height()];
    for y in 0..13 {
        for x in 0..99 {
            argb[(y * 108) + x] = image.pixel_argb(x, y).unwrap_or(0);
        }
        for x in 99..108 {
            argb[(y * 108) + x] = image.pixel_argb(x - 9, y).unwrap_or(0);
        }
        for x in 101..106 {
            argb[(y * 108) + x] = image.pixel_argb(x - 81, y).unwrap_or(0);
        }
    }

    XpmImage::from_argb_pixels(108, image.height(), argb)
        .expect("expanded numbers fallback dimensions are valid")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::io::Write;

    const ONE_PIXEL_XPM: &str = r#"
/* XPM */
static char * main_xpm[] = {
"1 1 1 1",
". c #010203",
"."};
"#;

    #[test]
    fn pixmap_info_matches_original_xmms_dimensions() {
        assert_eq!(
            SkinPixmapKind::Main.info().width,
            layout::MAIN_WINDOW_WIDTH as usize
        );
        assert_eq!(SkinPixmapKind::EqEx.info().height, 82);
        assert_eq!(SkinPixmapKind::Numbers.info().file_stem, "nums_ex");
    }

    #[test]
    fn bundled_default_skin_loads_without_filesystem_lookup() {
        let skin = DefaultSkin::load_bundled().unwrap();
        assert_eq!(skin.loaded_pixmap_count(), SkinPixmapKind::ALL.len());
        assert_eq!(
            skin.get(SkinPixmapKind::Main).unwrap().width(),
            SkinPixmapKind::Main.info().width
        );
        assert!(skin.get(SkinPixmapKind::Balance).is_some());
        assert_eq!(skin.vis_colors()[0], [9, 34, 53]);
        assert_eq!(skin.playlist_colors(), DEFAULT_PLAYLIST_COLORS);
    }

    #[test]
    fn partial_skin_uses_selected_equalizer_titlebar_for_shaded_fallback() {
        let width = layout::EQUALIZER_WINDOW_WIDTH as usize;
        let mut pixmaps = BTreeMap::from([
            (
                SkinPixmapKind::EqMain,
                XpmImage::from_argb_pixels(width, 116, vec![0xffff0000; width * 116]).unwrap(),
            ),
            (
                SkinPixmapKind::EqEx,
                XpmImage::from_argb_pixels(width, 82, vec![0xff0000ff; width * 82]).unwrap(),
            ),
        ]);
        let loaded = BTreeSet::from([SkinPixmapKind::EqMain]);

        apply_loaded_eq_ex_fallback(&mut pixmaps, &loaded);

        let fallback = pixmaps.get(&SkinPixmapKind::EqEx).unwrap();
        assert_eq!(fallback.pixel_argb(0, 0), Some(0xffff0000));
        assert_eq!(fallback.pixel_argb(0, 15), Some(0xffff0000));
        assert_eq!(fallback.pixel_argb(0, 30), Some(0xff0000ff));
    }

    #[test]
    fn directory_loader_accepts_png_skin_files() {
        let tmp = std::env::temp_dir().join(format!("xmms-rs-skin-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let mut image = image::RgbaImage::new(1, 1);
        image.put_pixel(0, 0, image::Rgba([48, 255, 50, 255]));
        image.save(tmp.join("main.png")).unwrap();

        let skin = DefaultSkin::load_from_dir(&tmp).unwrap();
        let main = skin.get(SkinPixmapKind::Main).unwrap();
        assert_eq!(main.width(), 1);
        assert_eq!(main.height(), 1);
        assert_eq!(main.pixel_argb(0, 0), Some(0));

        std::fs::remove_dir_all(tmp).unwrap();
    }

    #[test]
    fn directory_loader_preserves_default_pixmaps_for_partial_skins() {
        let tmp =
            std::env::temp_dir().join(format!("xmms-rs-skin-test-{}-partial", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("main.xpm"), ONE_PIXEL_XPM).unwrap();

        let skin = DefaultSkin::load_from_dir(&tmp).unwrap();
        let bundled = DefaultSkin::load_bundled().unwrap();

        assert_eq!(skin.loaded_pixmap_count(), SkinPixmapKind::ALL.len());
        assert_eq!(skin.get(SkinPixmapKind::Main).unwrap().width(), 1);
        assert_eq!(
            skin.get(SkinPixmapKind::Titlebar).unwrap().pixels_argb(),
            bundled.get(SkinPixmapKind::Titlebar).unwrap().pixels_argb()
        );

        std::fs::remove_dir_all(tmp).unwrap();
    }

    #[test]
    fn directory_loader_finds_skin_files_recursively_case_insensitively() {
        let tmp = std::env::temp_dir().join(format!(
            "xmms-rs-skin-test-{}-recursive",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&tmp);
        let nested = tmp.join("Nested");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("MAIN.XPM"), ONE_PIXEL_XPM).unwrap();
        std::fs::write(nested.join("VISCOLOR.TXT"), "7,8,9\n").unwrap();

        let skin = DefaultSkin::load_from_dir(&tmp).unwrap();

        assert_eq!(skin.get(SkinPixmapKind::Main).unwrap().width(), 1);
        assert_eq!(skin.vis_colors()[0], [7, 8, 9]);

        std::fs::remove_dir_all(tmp).unwrap();
    }

    #[test]
    fn directory_loader_prefers_top_level_files_before_nested_files() {
        let tmp = std::env::temp_dir().join(format!(
            "xmms-rs-skin-test-{}-recursive-prefer",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&tmp);
        let nested = tmp.join("Nested");
        std::fs::create_dir_all(&nested).unwrap();

        let mut top = image::RgbaImage::new(1, 1);
        top.put_pixel(0, 0, image::Rgba([1, 2, 3, 255]));
        top.save(tmp.join("main.png")).unwrap();
        let mut nested_image = image::RgbaImage::new(1, 1);
        nested_image.put_pixel(0, 0, image::Rgba([4, 5, 6, 255]));
        nested_image.save(nested.join("main.png")).unwrap();

        let skin = DefaultSkin::load_from_dir(&tmp).unwrap();

        assert_eq!(
            skin.get(SkinPixmapKind::Main).unwrap().pixel_argb(0, 0),
            Some(0xff010203)
        );

        std::fs::remove_dir_all(tmp).unwrap();
    }

    #[test]
    fn directory_loader_accepts_viscolor_overrides() {
        let tmp =
            std::env::temp_dir().join(format!("xmms-rs-skin-test-{}-viscolor", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("viscolor.txt"), "300,-1,42\n1 2 3\ninvalid\n").unwrap();

        let skin = DefaultSkin::load_from_dir(&tmp).unwrap();
        assert_eq!(skin.vis_colors()[0], [255, 0, 42]);
        assert_eq!(skin.vis_colors()[1], [1, 2, 3]);
        assert_eq!(skin.vis_colors()[2], DEFAULT_VIS_COLORS[2]);

        std::fs::remove_dir_all(tmp).unwrap();
    }

    #[test]
    fn directory_loader_accepts_playlist_color_overrides() {
        let tmp =
            std::env::temp_dir().join(format!("xmms-rs-skin-test-{}-pledit", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(
            tmp.join("pledit.txt"),
            "Normal=#010203\ncurrent=#A0B0C0\nNormalBG=#11223344\nSelectedBG=zz\n",
        )
        .unwrap();

        let skin = DefaultSkin::load_from_dir(&tmp).unwrap();
        assert_eq!(skin.playlist_colors().normal, [1, 2, 3]);
        assert_eq!(skin.playlist_colors().current, [160, 176, 192]);
        assert_eq!(skin.playlist_colors().normal_bg, [17, 34, 51]);
        assert_eq!(
            skin.playlist_colors().selected_bg,
            DEFAULT_PLAYLIST_COLORS.selected_bg
        );

        std::fs::remove_dir_all(tmp).unwrap();
    }

    #[test]
    fn playlist_color_parser_matches_original_ini_compatibility() {
        let colors = parse_playlist_colors(
            r#"
            [text]
            Normal=010203 ; comment
            CURRENT=#A0B0C0
            normalbg=1234
            selectedbg=#56
            "#,
        );

        assert_eq!(colors.normal, [1, 2, 3]);
        assert_eq!(colors.current, [160, 176, 192]);
        assert_eq!(colors.normal_bg, [0, 0x12, 0x34]);
        assert_eq!(colors.selected_bg, [0, 0, 0x56]);
    }

    #[test]
    fn playlist_color_parser_keeps_defaults_for_invalid_values() {
        let colors = parse_playlist_colors(
            r#"
            [text]
            Normal=zzzzzz
            Current=#
            NormalBG=1
            SelectedBG=#010203
            "#,
        );

        assert_eq!(colors.normal, DEFAULT_PLAYLIST_COLORS.normal);
        assert_eq!(colors.current, DEFAULT_PLAYLIST_COLORS.current);
        assert_eq!(colors.normal_bg, DEFAULT_PLAYLIST_COLORS.normal_bg);
        assert_eq!(colors.selected_bg, [1, 2, 3]);
    }

    #[test]
    fn parses_region_masks_for_all_original_sections() {
        let masks = parse_region_masks(
            r#"
            [Normal]
            NumPoints=3
            PointList=0,0, 10,0, 10,10
            [WindowShade]
            NumPoints=2
            PointList=1,2,3,4
            [Equalizer]
            NumPoints=4
            PointList=0,0,5,0,5,5,0,5
            [EqualizerWS]
            NumPoints=2
            PointList=6,7,8,9
            "#,
        );

        assert_eq!(
            masks.normal.unwrap().polygons(),
            &[vec![[0, 0], [10, 0], [10, 10]]]
        );
        assert_eq!(
            masks.window_shade.unwrap().scaled_polygons(true),
            vec![vec![[2, 4], [6, 8]]]
        );
        assert!(masks.equalizer.is_some());
        assert!(masks.equalizer_ws.is_some());
    }

    #[test]
    fn region_masks_ignore_missing_and_malformed_sections() {
        let masks = parse_region_masks(
            r#"
            [Normal]
            NumPoints=3
            PointList=1,2
            [Equalizer]
            PointList=0,0,1,1
            "#,
        );

        assert_eq!(masks, RegionMasks::default());
    }

    #[test]
    fn directory_loader_accepts_nested_region_masks() {
        let tmp =
            std::env::temp_dir().join(format!("xmms-rs-skin-test-{}-region", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let nested = tmp.join("Nested");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(
            nested.join("REGION.TXT"),
            "[Normal]\nNumPoints=2\nPointList=1,2,3,4\n",
        )
        .unwrap();

        let skin = DefaultSkin::load_from_dir(&tmp).unwrap();

        assert_eq!(
            skin.region_masks().normal.as_ref().unwrap().polygons(),
            &[vec![[1, 2], [3, 4]]]
        );

        std::fs::remove_dir_all(tmp).unwrap();
    }

    #[test]
    fn directory_loader_expands_legacy_numbers_fallback() {
        let tmp =
            std::env::temp_dir().join(format!("xmms-rs-skin-test-{}-numbers", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let mut image = image::RgbaImage::new(99, 13);
        image.put_pixel(20, 6, image::Rgba([1, 2, 3, 255]));
        image.put_pixel(90, 6, image::Rgba([4, 5, 6, 255]));
        image.save(tmp.join("numbers.png")).unwrap();

        let skin = DefaultSkin::load_from_dir(&tmp).unwrap();
        let numbers = skin.get(SkinPixmapKind::Numbers).unwrap();
        assert_eq!(numbers.width(), 108);
        assert_eq!(numbers.height(), 13);
        assert_eq!(numbers.pixel_argb(99, 6), Some(0xff040506));
        assert_eq!(numbers.pixel_argb(101, 6), Some(0xff010203));

        std::fs::remove_dir_all(tmp).unwrap();
    }

    #[test]
    fn directory_loader_uses_volume_as_balance_fallback() {
        let tmp =
            std::env::temp_dir().join(format!("xmms-rs-skin-test-{}-balance", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let mut image = image::RgbaImage::new(1, 1);
        image.put_pixel(0, 0, image::Rgba([10, 20, 30, 255]));
        image.save(tmp.join("volume.png")).unwrap();

        let skin = DefaultSkin::load_from_dir(&tmp).unwrap();
        let balance = skin.get(SkinPixmapKind::Balance).unwrap();
        assert_eq!(balance.width(), 1);
        assert_eq!(balance.height(), 1);
        assert_eq!(balance.pixel_argb(0, 0), Some(0xff0a141e));

        std::fs::remove_dir_all(tmp).unwrap();
    }

    #[test]
    fn directory_loader_derives_text_colors_from_text_pixmap() {
        let tmp = std::env::temp_dir().join(format!(
            "xmms-rs-skin-test-{}-text-colors",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let mut image = image::RgbaImage::new(155, 6);
        for y in 0..6 {
            for x in 0..155 {
                image.put_pixel(x, y, image::Rgba([10, 20, 30, 255]));
            }
            image.put_pixel(1, y, image::Rgba([200, 210, 220, 255]));
            image.put_pixel(151, y, image::Rgba([1, 2, 3 + y as u8, 255]));
        }
        image.save(tmp.join("text.png")).unwrap();

        let skin = DefaultSkin::load_from_dir(&tmp).unwrap();
        let colors = skin.text_colors();

        assert_eq!(colors.background[0], [1, 2, 3]);
        assert_eq!(colors.background[5], [1, 2, 8]);
        assert_eq!(colors.foreground[0], [200, 210, 220]);
        assert_eq!(colors.foreground[5], [200, 210, 220]);

        std::fs::remove_dir_all(tmp).unwrap();
    }

    #[test]
    fn path_loader_accepts_wsz_zip_skin_archives() {
        let path =
            std::env::temp_dir().join(format!("xmms-rs-skin-test-{}-zip.wsz", std::process::id()));
        let _ = std::fs::remove_file(&path);

        let file = File::create(&path).unwrap();
        let mut archive = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        archive.start_file("Example/Main.xpm", options).unwrap();
        archive.write_all(ONE_PIXEL_XPM.as_bytes()).unwrap();
        archive.start_file("Example/viscolor.txt", options).unwrap();
        archive.write_all(b"4,5,6\n").unwrap();
        archive.start_file("Example/pledit.txt", options).unwrap();
        archive.write_all(b"Normal=#070809\n").unwrap();
        archive.finish().unwrap();

        let skin = DefaultSkin::load_from_path(&path).unwrap();
        let main = skin.get(SkinPixmapKind::Main).unwrap();
        assert_eq!(main.width(), 1);
        assert_eq!(main.height(), 1);
        assert_eq!(skin.vis_colors()[0], [4, 5, 6]);
        assert_eq!(skin.playlist_colors().normal, [7, 8, 9]);

        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn archive_loader_preserves_default_pixmaps_for_partial_skins() {
        let path = std::env::temp_dir().join(format!(
            "xmms-rs-skin-test-{}-partial.wsz",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);

        let file = File::create(&path).unwrap();
        let mut archive = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        archive.start_file("Example/Main.xpm", options).unwrap();
        archive.write_all(ONE_PIXEL_XPM.as_bytes()).unwrap();
        archive.finish().unwrap();

        let skin = DefaultSkin::load_from_path(&path).unwrap();
        let bundled = DefaultSkin::load_bundled().unwrap();

        assert_eq!(skin.loaded_pixmap_count(), SkinPixmapKind::ALL.len());
        assert_eq!(skin.get(SkinPixmapKind::Main).unwrap().width(), 1);
        assert_eq!(
            skin.get(SkinPixmapKind::EqMain).unwrap().pixels_argb(),
            bundled.get(SkinPixmapKind::EqMain).unwrap().pixels_argb()
        );

        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn path_loader_accepts_tar_gz_skin_archives() {
        let path = std::env::temp_dir().join(format!(
            "xmms-rs-skin-test-{}-tar.tar.gz",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);

        let file = File::create(&path).unwrap();
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut archive = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_size(ONE_PIXEL_XPM.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        archive
            .append_data(&mut header, "Example/main.xpm", Cursor::new(ONE_PIXEL_XPM))
            .unwrap();
        archive.into_inner().unwrap().finish().unwrap();

        let skin = DefaultSkin::load_from_path(&path).unwrap();
        let main = skin.get(SkinPixmapKind::Main).unwrap();
        assert_eq!(main.width(), 1);
        assert_eq!(main.height(), 1);

        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn archive_discovery_matches_loader_supported_extensions() {
        assert!(is_skin_archive_path(Path::new("Example.zip")));
        assert!(is_skin_archive_path(Path::new("Example.wsz")));
        assert!(is_skin_archive_path(Path::new("Example.tar")));
        assert!(is_skin_archive_path(Path::new("Example.tar.gz")));
        assert!(is_skin_archive_path(Path::new("Example.tgz")));
        assert!(is_skin_archive_path(Path::new("Example.tar.bz2")));
        assert!(is_skin_archive_path(Path::new("Example.tbz2")));
        assert!(!is_skin_archive_path(Path::new("Example.gz")));
        assert!(!is_skin_archive_path(Path::new("Example.bz2")));
        assert!(!is_skin_archive_path(Path::new("Example.txt")));
    }

    #[test]
    fn path_loader_accepts_every_discovered_archive_extension() {
        let root =
            std::env::temp_dir().join(format!("xmms-rs-skin-test-{}-archives", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        for extension in ["zip", "wsz", "tar", "tar.gz", "tgz", "tar.bz2", "tbz2"] {
            let path = root.join(format!("Example.{extension}"));
            write_test_skin_archive(&path, ONE_PIXEL_XPM.as_bytes()).unwrap();
            let skin = DefaultSkin::load_from_path(&path).unwrap();
            assert_eq!(
                skin.get(SkinPixmapKind::Main).unwrap().pixel_argb(0, 0),
                Some(0xff010203),
                "{extension}"
            );
        }

        std::fs::remove_dir_all(root).unwrap();
    }

    fn write_test_skin_archive(path: &Path, contents: &[u8]) -> io::Result<()> {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();

        if name.ends_with(".zip") || name.ends_with(".wsz") {
            let file = File::create(path)?;
            let mut archive = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            archive.start_file("Example/main.xpm", options)?;
            archive.write_all(contents)?;
            archive.finish()?;
            return Ok(());
        }

        if name.ends_with(".tar") {
            let file = File::create(path)?;
            let mut archive = tar::Builder::new(file);
            append_test_skin_tar_entry(&mut archive, contents)?;
            archive.finish()?;
            return Ok(());
        }

        if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
            let file = File::create(path)?;
            let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
            let mut archive = tar::Builder::new(encoder);
            append_test_skin_tar_entry(&mut archive, contents)?;
            archive.into_inner()?.finish()?;
            return Ok(());
        }

        if name.ends_with(".tar.bz2") || name.ends_with(".tbz2") {
            let file = File::create(path)?;
            let encoder = bzip2::write::BzEncoder::new(file, bzip2::Compression::default());
            let mut archive = tar::Builder::new(encoder);
            append_test_skin_tar_entry(&mut archive, contents)?;
            archive.into_inner()?.finish()?;
            return Ok(());
        }

        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unsupported test archive: {}", path.display()),
        ))
    }

    fn append_test_skin_tar_entry<W: Write>(
        archive: &mut tar::Builder<W>,
        contents: &[u8],
    ) -> io::Result<()> {
        let mut header = tar::Header::new_gnu();
        header.set_size(contents.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        archive.append_data(&mut header, "Example/main.xpm", Cursor::new(contents))
    }
}
