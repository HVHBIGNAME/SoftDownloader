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
    fn new(size: u32, bgra: Vec<u8>) -> Option<Self> {
        let bytes = bgra.len();
        if size == 0 || bytes != (size as usize) * (size as usize) * 4 {
            return None;
        }
        let mut rgba = bgra;
        // 32-bit DIB sections carry an unreliable alpha channel; GDI icons are
        // opaque unless the application ships an alpha mask of its own.
        for offset in (0..rgba.len()).step_by(4) {
            rgba.swap(offset, offset + 2);
            if rgba[offset + 3] == 0 {
                rgba[offset + 3] = 255;
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
        Foundation::HANDLE,
        Graphics::Gdi::{
            BI_RGB, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits,
            ReleaseDC,
        },
        UI::{
            Shell::ExtractIconExW,
            WindowsAndMessaging::{DestroyIcon, GetIconInfo, ICONINFO, PrivateExtractIconsW},
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
        let image = unsafe { bitmap_from_icon(icon, size) };
        unsafe { DestroyIcon(icon) };
        image
    }

    fn system_icon(name: &[u16], size: u32) -> Option<IconImage> {
        let mut large = null_mut();
        let mut small = null_mut();
        let count = unsafe { ExtractIconExW(name.as_ptr(), 0, &mut large, &mut small, 1) };
        if count == 0 {
            return None;
        }
        // ExtractIconExW only exposes the fixed 32 and 16 pixel handles.
        let (icon, actual) = if size > 32 && !large.is_null() {
            (large, 32)
        } else if !small.is_null() {
            (small, 16)
        } else {
            (large, 32)
        };
        let image = unsafe { bitmap_from_icon(icon, actual) };
        unsafe {
            if !large.is_null() {
                DestroyIcon(large);
            }
            if !small.is_null() {
                DestroyIcon(small);
            }
        }
        image
    }

    unsafe fn bitmap_from_icon(icon: HANDLE, size: u32) -> Option<IconImage> {
        let mut info = ICONINFO::default();
        if unsafe { GetIconInfo(icon, &mut info) } == 0 {
            return None;
        }
        let bitmap = if info.hbmColor.is_null() {
            info.hbmMask
        } else {
            info.hbmColor
        };
        if bitmap.is_null() {
            return None;
        }
        let screen = unsafe { GetDC(null_mut()) };
        if screen.is_null() {
            return None;
        }
        let mut header = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size as i32,
                // A negative height requests a top-down DIB matching image order.
                biHeight: -(size as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = vec![0_u8; (size as usize) * (size as usize) * 4];
        let read = unsafe {
            GetDIBits(
                screen,
                bitmap,
                0,
                size,
                pixels.as_mut_ptr() as *mut c_void,
                &mut header,
                DIB_RGB_COLORS,
            )
        };
        unsafe {
            ReleaseDC(null_mut(), screen);
            DeleteObject(bitmap);
            DeleteObject(info.hbmMask);
            if !info.hbmColor.is_null() {
                DeleteObject(info.hbmColor);
            }
        }
        if read == 0 {
            return None;
        }
        IconImage::new(size, pixels)
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
        assert!(IconImage::new(0, Vec::new()).is_none());
        assert!(IconImage::new(2, vec![0; 8]).is_none());
        let image = IconImage::new(1, vec![1, 2, 3, 0]).unwrap();
        assert_eq!(&image.rgba, &[3, 2, 1, 255], "BGRA is swapped to RGBA");
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
            .any(|at| image.rgba[at + 3] == 255 && image.rgba[at] > image.rgba[at + 1]);
        assert!(visible, "the extracted icon must contain visible pixels");
    }
}
