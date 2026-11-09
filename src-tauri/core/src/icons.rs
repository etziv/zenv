//! Logos for languages and servers, taken from their own installations:
//! the icon embedded in the executable (php.exe, python.exe, ruby.exe, …) or
//! an image the distribution ships (Go's gopher, Rust's logo when its docs are
//! installed). Cached as files under `<home>\cache\icons`.

use std::path::{Path, PathBuf};

use crate::error::AppResult;
use crate::paths::AppPaths;
use crate::platform;
use crate::runtime_catalog::ComponentId;
use crate::runtime_manager::{self, Installation};
use crate::storage::Db;
use crate::thumbnails::base64;

const SIZE: u32 = 64;

/// Images some distributions ship, relative to the installation root.
fn shipped_images(component: ComponentId, root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    match component {
        ComponentId::Go => out.push(root.join(r"misc\chrome\gophertool\gopher.png")),
        ComponentId::Rust => {
            // rustup layout: <root>\rustup\toolchains\<name>\share\doc\rust\html\static.files\rust-logo-*.svg
            for toolchains in [root.join(r"rustup\toolchains"), root.join(r"..\.rustup\toolchains")] {
                for tc in std::fs::read_dir(&toolchains).into_iter().flatten().flatten() {
                    let dir = tc.path().join(r"share\doc\rust\html\static.files");
                    for f in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                        let name = f.file_name().to_string_lossy().to_ascii_lowercase();
                        if name.starts_with("rust-logo") && name.ends_with(".svg") {
                            out.push(f.path());
                        }
                    }
                }
            }
        }
        // npm packages run by node.exe: the logo their dashboard uses.
        ComponentId::Omniroute => out.extend([
            root.join(r"node_modules\omniroute\dist\public\favicon.svg"),
            root.join(r"node_modules\omniroute\public\favicon.svg"),
        ]),
        ComponentId::Ninerouter => out.push(root.join(r"node_modules\9router\app\public\favicon.svg")),
        _ => {}
    }
    out.into_iter().filter(|p| p.is_file()).collect()
}

fn mime(path: &Path) -> &'static str {
    match path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).as_deref() {
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        _ => "image/png",
    }
}

fn data_url(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    Some(format!("data:{};base64,{}", mime(path), base64(&bytes)))
}

// ----- a minimal PNG writer (stored deflate blocks; icons are tiny) ----------

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xedb8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data {
        a = (a + u32::from(x)) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

pub fn png_rgba(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity(rgba.len() + height as usize);
    for row in rgba.chunks_exact(width as usize * 4) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut zlib = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
    for (i, block) in blocks.iter().enumerate() {
        zlib.push(u8::from(i + 1 == blocks.len()));
        let len = block.len() as u16;
        zlib.extend_from_slice(&len.to_le_bytes());
        zlib.extend_from_slice(&(!len).to_le_bytes());
        zlib.extend_from_slice(block);
    }
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut chunk = |kind: &[u8; 4], data: &[u8]| {
        png.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        png.extend_from_slice(&body);
        png.extend_from_slice(&crc32(&body).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &zlib);
    chunk(b"IEND", &[]);
    png
}

// ----- lookup --------------------------------------------------------------------

fn cache_dir(paths: &AppPaths) -> PathBuf {
    paths.root.join("cache").join("icons")
}

/// The installation's logo, extracted once and cached.
pub fn installation_icon(paths: &AppPaths, install: &Installation) -> Option<String> {
    let dir = cache_dir(paths);
    for ext in ["png", "svg"] {
        let cached = dir.join(format!("{}.{ext}", install.id));
        if cached.is_file() {
            return data_url(&cached);
        }
    }
    let _ = std::fs::create_dir_all(&dir);
    if let Some(rgba) = platform::exe_icon_rgba(&install.executable_path, SIZE) {
        let file = dir.join(format!("{}.png", install.id));
        if std::fs::write(&file, png_rgba(SIZE, SIZE, &rgba)).is_ok() {
            return data_url(&file);
        }
    }
    let image = shipped_images(install.component, &install.root_path).into_iter().next()?;
    let ext = if mime(&image) == "image/svg+xml" { "svg" } else { "png" };
    let file = dir.join(format!("{}.{ext}", install.id));
    std::fs::copy(&image, &file).ok()?;
    data_url(&file)
}

/// The icon embedded in a program (an editor, say), cached under `key`.
pub fn program_icon(paths: &AppPaths, key: &str, exe: &Path) -> Option<String> {
    let dir = cache_dir(paths);
    let cached = dir.join(format!("{key}.png"));
    if !cached.is_file() {
        let rgba = platform::exe_icon_rgba(exe, SIZE)?;
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(&cached, png_rgba(SIZE, SIZE, &rgba)).ok()?;
    }
    data_url(&cached)
}

/// A logo for a component: from its default installation first, then any other.
pub fn component_icon(db: &Db, paths: &AppPaths, component: ComponentId) -> AppResult<Option<String>> {
    let mut installs = runtime_manager::list(db, Some(component))?;
    installs.retain(|i| i.executable_exists);
    installs.sort_by_key(|i| !i.is_default);
    Ok(installs.iter().find_map(|i| installation_icon(paths, i)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_a_valid_png() {
        // 2x1: one opaque red pixel, one transparent.
        let png = png_rgba(2, 1, &[255, 0, 0, 255, 0, 0, 0, 0]);
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert_eq!(&png[12..16], b"IHDR");
        assert!(png.ends_with(&[0xae, 0x42, 0x60, 0x82]), "standard IEND CRC");
        assert_eq!(crc32(b"IEND"), 0xae42_6082);
        assert_eq!(adler32(b"Wikipedia"), 0x11e6_0398);
    }

    #[test]
    fn routers_use_their_dashboard_logo() {
        let dir = tempfile::tempdir().unwrap();
        let logo = dir.path().join(r"node_modules\9router\app\public\favicon.svg");
        std::fs::create_dir_all(logo.parent().unwrap()).unwrap();
        std::fs::write(&logo, "<svg/>").unwrap();
        assert_eq!(shipped_images(ComponentId::Ninerouter, dir.path()), vec![logo]);
        assert!(shipped_images(ComponentId::Omniroute, dir.path()).is_empty(), "nothing made up");
    }

    #[test]
    #[cfg(windows)]
    fn extracts_an_icon_from_a_windows_executable() {
        let notepad = PathBuf::from(std::env::var("WINDIR").unwrap()).join(r"System32\notepad.exe");
        let rgba = platform::exe_icon_rgba(&notepad, SIZE).expect("notepad has an icon");
        assert_eq!(rgba.len(), (SIZE * SIZE * 4) as usize);
        assert!(rgba.chunks_exact(4).any(|p| p[3] > 0), "has visible pixels");
    }
}
