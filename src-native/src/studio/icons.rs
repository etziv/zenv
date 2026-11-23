use eframe::egui::{self, pos2, Rect, Vec2};

use super::icon_map::{self as map, CELL, COLUMNS, COUNT};

const ATLAS: &[u8] = include_bytes!("../../assets/file-icons.png");

pub fn file_icon(name: &str) -> u16 {
    let lower = name.to_lowercase();
    if let Ok(i) = map::NAMES.binary_search_by(|(n, _)| (*n).cmp(lower.as_str())) {
        return map::NAMES[i].1;
    }
    if lower.starts_with(".env") {
        return map::TUNE;
    }
    if lower.starts_with("dockerfile") {
        return map::DOCKER;
    }
    let mut rest = lower.as_str();
    while let Some((_, ext)) = rest.split_once('.') {
        if let Ok(i) = map::EXTENSIONS.binary_search_by(|(e, _)| (*e).cmp(ext)) {
            return map::EXTENSIONS[i].1;
        }
        rest = ext;
    }
    map::FILE
}

pub fn folder_icon(name: &str, open: bool) -> u16 {
    let lower = name.to_lowercase();
    let (closed, opened) = map::FOLDERS.binary_search_by(|(n, ..)| (*n).cmp(lower.as_str())).map_or(map::FOLDER, |i| (map::FOLDERS[i].1, map::FOLDERS[i].2));
    if open { opened } else { closed }
}

pub fn icon_name(icon: u16) -> &'static str {
    map::ICON_NAMES[icon as usize]
}

pub fn image(icon: u16, size: f32) -> egui::Image<'static> {
    let rows = COUNT.div_ceil(COLUMNS);
    let (col, row) = (u32::from(icon) % COLUMNS, u32::from(icon) / COLUMNS);
    let (w, h) = ((COLUMNS * CELL) as f32, (rows * CELL) as f32);
    let uv = Rect::from_min_max(pos2((col * CELL) as f32 / w, (row * CELL) as f32 / h), pos2(((col + 1) * CELL) as f32 / w, ((row + 1) * CELL) as f32 / h));
    egui::Image::from_bytes("bytes://zenv-file-icons.png", ATLAS).uv(uv).fit_to_exact_size(Vec2::splat(size))
}
