//! Reads real program icons out of installed Windows executables.
//!
//! Icons are taken from the application itself, so no third-party artwork is
//! bundled or downloaded. Nothing is executed and the source file is only read.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IconImage {
    pub size: u32,
    pub rgba: Vec<u8>,
}

impl IconImage {
    fn from_backgrounds(size: u32, black: &[u8], white: &[u8]) -> Option<Self> {
        let expected = (size as usize).checked_mul(size as usize)?.checked_mul(4)?;
        if size == 0 || black.len() != expected || white.len() != expected {
            return None;
        }
        let mut rgba = vec![0; expected];
        // Rendering on both backgrounds recovers alpha for legacy AND masks
        // as well as premultiplied 32-bit icons: white - black = 1 - alpha.
        for offset in (0..expected).step_by(4) {
            let alpha = 255 - white[offset].saturating_sub(black[offset]);
            rgba[offset + 3] = alpha;
            if alpha != 0 {
                for (rgb, bgr) in [(0, 2), (1, 1), (2, 0)] {
                    rgba[offset + rgb] = ((black[offset + bgr] as u32 * 255 + alpha as u32 / 2)
                        / alpha as u32)
                        .min(255) as u8;
                }
            }
        }
        Some(Self { size, rgba })
    }
}

/// Extracts the first icon of `path` scaled to `size` pixels.
///
/// Returns `None` on other platforms, for non-image files and when Windows
/// cannot supply an icon. Sizes are limited to what the UI actually renders.
pub fn from_executable(path: &Path, size: u32) -> Option<IconImage> {
    platform::extract(path, size.clamp(16, 128))
}

/// Picks the executable that best represents a managed package folder.
///
/// The package marker is always skipped, and small helper binaries lose to the
/// largest launcher so the icon matches the program the user actually starts.
pub fn primary_file(folder: &Path) -> Option<PathBuf> {
    let mut fallback: Option<(u64, PathBuf)> = None;
    for entry in std::fs::read_dir(folder).ok()?.flatten() {
        let path = entry.path();
        if path
            .extension()
            .is_none_or(|extension| !extension.eq_ignore_ascii_case("exe"))
        {
            continue;
        }
        let stem = path.file_stem()?.to_string_lossy().to_ascii_lowercase();
        if stem.starts_with("unins") || stem.starts_with("crashpad") || stem.contains("updater") {
            continue;
        }
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        if size > fallback.as_ref().map_or(0, |(best, _)| *best) {
            fallback = Some((size, path));
        }
    }
    fallback.map(|(_, path)| path)
}

#[cfg(windows)]
mod platform {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use std::ptr::null_mut;

    use windows_sys::Win32::{
        Graphics::Gdi::{
            BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection,
            DIB_RGB_COLORS, DeleteDC, DeleteObject, GdiFlush, HBITMAP, HDC, HGDIOBJ, SelectObject,
        },
        UI::{
            Shell::ExtractIconExW,
            WindowsAndMessaging::{
                DI_NORMAL, DestroyIcon, DrawIconEx, HICON, PrivateExtractIconsW,
            },
        },
    };

    use super::IconImage;

    pub(super) fn extract(path: &Path, size: u32) -> Option<IconImage> {
        if !path.is_file() {
            return None;
        }
        let name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        // PrivateExtractIconsW rescales the application icon, unlike the fixed
        // 16/32 pixel handles of ExtractIconExW.
        let mut icon = null_mut();
        let found = unsafe {
            PrivateExtractIconsW(
                name.as_ptr(),
                0,
                size as i32,
                size as i32,
                &mut icon,
                null_mut(),
                1,
                0,
            )
        };
        if found == 0 || icon.is_null() {
            return system_icon(&name, size);
        }
        render_icon(&OwnedIcon(icon), size)
    }

    fn system_icon(name: &[u16], size: u32) -> Option<IconImage> {
        let mut large = null_mut();
        let mut small = null_mut();
        let count = unsafe { ExtractIconExW(name.as_ptr(), 0, &mut large, &mut small, 1) };
        if count == 0 {
            return None;
        }
        let large = OwnedIcon(large);
        let small = OwnedIcon(small);
        render_icon(if large.0.is_null() { &small } else { &large }, size)
    }

    struct OwnedIcon(HICON);

    impl Drop for OwnedIcon {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    DestroyIcon(self.0);
                }
            }
        }
    }

    struct Surface {
        dc: HDC,
        bitmap: HBITMAP,
        previous: HGDIOBJ,
        pixels: *mut c_void,
        size: u32,
    }

    impl Surface {
        fn new(size: u32) -> Option<Self> {
            let header = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: size as i32,
                    biHeight: -(size as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB,
                    ..Default::default()
                },
                ..Default::default()
            };
            let dc = unsafe { CreateCompatibleDC(null_mut()) };
            if dc.is_null() {
                return None;
            }
            let mut pixels = null_mut();
            let bitmap = unsafe {
                CreateDIBSection(dc, &header, DIB_RGB_COLORS, &mut pixels, null_mut(), 0)
            };
            if bitmap.is_null() {
                unsafe {
                    DeleteDC(dc);
                }
                return None;
            }
            let previous = unsafe { SelectObject(dc, bitmap) };
            let surface = Self {
                dc,
                bitmap,
                previous,
                pixels,
                size,
            };
            if surface.pixels.is_null() || previous.is_null() || previous as isize == -1 {
                return None;
            }
            Some(surface)
        }

        fn render(&mut self, icon: &OwnedIcon, background: u8) -> Option<Vec<u8>> {
            let length = (self.size * self.size * 4) as usize;
            // The DIB owns exactly size*size 32-bit pixels and outlives this slice.
            unsafe {
                std::slice::from_raw_parts_mut(self.pixels.cast::<u8>(), length).fill(background);
            }
            let drawn = unsafe {
                DrawIconEx(
                    self.dc,
                    0,
                    0,
                    icon.0,
                    self.size as i32,
                    self.size as i32,
                    0,
                    null_mut(),
                    DI_NORMAL,
                )
            };
            if drawn == 0 {
                return None;
            }
            if unsafe { GdiFlush() } == 0 {
                return None;
            }
            Some(unsafe { std::slice::from_raw_parts(self.pixels.cast::<u8>(), length) }.to_vec())
        }
    }

    impl Drop for Surface {
        fn drop(&mut self) {
            unsafe {
                if !self.previous.is_null() && self.previous as isize != -1 {
                    SelectObject(self.dc, self.previous);
                }
                DeleteObject(self.bitmap);
                DeleteDC(self.dc);
            }
        }
    }

    fn render_icon(icon: &OwnedIcon, size: u32) -> Option<IconImage> {
        if icon.0.is_null() {
            return None;
        }
        let mut surface = Surface::new(size)?;
        let black = surface.render(icon, 0)?;
        let white = surface.render(icon, 255)?;
        IconImage::from_backgrounds(size, &black, &white)
    }
}

#[cfg(not(windows))]
mod platform {
    use std::path::Path;

    use super::IconImage;

    pub(super) fn extract(_path: &Path, _size: u32) -> Option<IconImage> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_missing_files_and_odd_sizes() {
        assert!(from_executable(Path::new(r"C:\missing\program.exe"), 32).is_none());
        assert!(IconImage::from_backgrounds(0, &[], &[]).is_none());
        assert!(IconImage::from_backgrounds(2, &[0; 8], &[0; 8]).is_none());
        let image = IconImage::from_backgrounds(1, &[1, 2, 3, 0], &[1, 2, 3, 255]).unwrap();
        assert_eq!(&image.rgba, &[3, 2, 1, 255], "BGRA is swapped to RGBA");
    }

    #[test]
    fn keeps_transparent_and_translucent_pixels() {
        let transparent =
            IconImage::from_backgrounds(1, &[0, 0, 0, 0], &[255, 255, 255, 0]).unwrap();
        assert_eq!(transparent.rgba, [0, 0, 0, 0]);
        let translucent =
            IconImage::from_backgrounds(1, &[10, 20, 30, 0], &[137, 147, 157, 0]).unwrap();
        assert_eq!(translucent.rgba, [60, 40, 20, 128]);
    }

    #[test]
    fn the_main_launcher_wins_over_helpers_in_a_package_folder() {
        let folder = tempfile::tempdir().unwrap();
        let write = |name: &str, size: usize| {
            let path = folder.path().join(name);
            std::fs::write(&path, vec![0_u8; size]).unwrap();
            path
        };
        let launcher = write("program.exe", 4096);
        write("uninstall.exe", 900_000);
        write("updater.exe", 800_000);
        write("crashpad-handler.exe", 700_000);
        assert_eq!(
            primary_file(folder.path()).as_deref(),
            Some(launcher.as_path())
        );
    }

    #[test]
    fn a_folder_without_launchers_yields_no_icon() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("readme.txt"), b"no executables").unwrap();
        assert!(primary_file(folder.path()).is_none());
        assert!(primary_file(Path::new(r"C:\missing\folder")).is_none());
    }

    #[cfg(windows)]
    #[test]
    fn requested_sizes_are_clamped_to_what_the_interface_renders() {
        let directory = crate::installer::windows::system_directory().unwrap();
        let executable = ["mmc.exe", "notepad.exe", "cmd.exe"]
            .into_iter()
            .map(|name| directory.join(name))
            .find(|path| path.is_file())
            .unwrap();
        for requested in [1, 16, 64, 4096] {
            let image = from_executable(&executable, requested).expect("system icon");
            assert!(
                (16..=128).contains(&image.size),
                "size {requested} produced {}",
                image.size
            );
            assert_eq!(
                image.rgba.len(),
                (image.size * image.size * 4) as usize,
                "pixel buffer must match the reported size"
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn reads_the_icon_of_a_windows_system_binary() {
        // Well-known Windows binaries with embedded icon groups. The list is
        // probed so the test survives trimmed-down installations.
        let directory = crate::installer::windows::system_directory().unwrap();
        let image = ["mmc.exe", "notepad.exe", "cmd.exe", "taskmgr.exe"]
            .into_iter()
            .find_map(|name| from_executable(&directory.join(name), 32))
            .expect("Windows binaries expose their icon");
        assert_eq!(image.size, 32);
        assert_eq!(image.rgba.len(), 32 * 32 * 4);
        let visible = (0..image.rgba.len())
            .step_by(4)
            .any(|at| image.rgba[at + 3] > 0);
        assert!(visible, "the extracted icon must contain visible pixels");
    }
}
