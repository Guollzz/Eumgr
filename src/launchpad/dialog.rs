//! The native pickers of the launchpad.
//!
//! Both dialogs come straight from Win32 so the application does not need a new
//! dependency for a file chooser:
//!
//! * files  -> `GetOpenFileNameW` (comdlg32), with multi selection enabled,
//! * folders -> `SHBrowseForFolderW` (shell32), the only folder picker that
//!   works without an OLE apartment.
//!
//! Both functions block until the user closed the dialog, which is exactly what
//! a modal picker should do; the caller refreshes the UI afterwards.
//!
//! Every call writes one log line (including the case where the user cancelled),
//! because "nothing happened" is otherwise indistinguishable from a bug.

use crate::logs;
use std::ffi::c_void;

/// Log location shared by every line of this module.
const LOG_LOCATION: &str = "launchpad::dialog";

/// Turns a Rust string into a NUL terminated UTF-16 buffer for the `W` APIs.
#[cfg(windows)]
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Opens the multi selection file picker and returns the chosen paths.
///
/// An empty vector means "cancelled" (or a platform without Win32); a real
/// dialog error is logged with the code of `CommDlgExtendedError`.
#[cfg(windows)]
pub fn pick_files(owner: *mut c_void, title: &str) -> Vec<String> {
    // `OPENFILENAMEW` is 152 bytes on 64-bit Windows. The layout below mirrors
    // the C struct field by field, `#[repr(C)]` adds the same padding MSVC does.
    #[repr(C)]
    #[derive(Default)]
    struct OpenFileNameW {
        l_struct_size: u32,
        hwnd_owner: *mut c_void,
        h_instance: *mut c_void,
        lpstr_filter: *const u16,
        lpstr_custom_filter: *mut u16,
        n_max_cust_filter: u32,
        n_filter_index: u32,
        lpstr_file: *mut u16,
        n_max_file: u32,
        lpstr_file_title: *mut u16,
        n_max_file_title: u32,
        lpstr_initial_dir: *const u16,
        lpstr_title: *const u16,
        flags: u32,
        n_file_offset: u16,
        n_file_extension: u16,
        lpstr_def_ext: *const u16,
        l_cust_data: isize,
        lpfn_hook: *mut c_void,
        lp_template_name: *const u16,
        pv_reserved: *mut c_void,
        dw_reserved: u32,
        flags_ex: u32,
    }

    #[link(name = "comdlg32")]
    unsafe extern "system" {
        /// Shows the classic open dialog; returns 0 on cancel/error.
        fn GetOpenFileNameW(dialog: *mut OpenFileNameW) -> i32;
        /// Last dialog error, 0 when the user simply cancelled.
        fn CommDlgExtendedError() -> u32;
    }

    // OFN_EXPLORER | OFN_ALLOWMULTISELECT | OFN_FILEMUSTEXIST |
    // OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR.
    //
    // OFN_NOCHANGEDIR matters: without it the dialog changes the working
    // directory of the process, and every relative path (including
    // `config.json`) would afterwards point somewhere else.
    const FLAGS: u32 = 0x0008_0000 | 0x0000_0200 | 0x0000_1000 | 0x0000_0800 | 0x0000_0008;

    /// Room for many long paths at once; the API writes NUL separated names.
    const BUFFER_LEN: usize = 32 * 1024;

    let title_wide = wide(title);
    let mut buffer = vec![0u16; BUFFER_LEN];

    let mut dialog = OpenFileNameW {
        hwnd_owner: owner,
        lpstr_title: title_wide.as_ptr(),
        lpstr_file: buffer.as_mut_ptr(),
        n_max_file: BUFFER_LEN as u32,
        flags: FLAGS,
        n_filter_index: 1,
        ..Default::default()
    };
    dialog.l_struct_size = std::mem::size_of::<OpenFileNameW>() as u32;

    let accepted = unsafe { GetOpenFileNameW(&mut dialog) };
    if accepted == 0 {
        let error = unsafe { CommDlgExtendedError() };
        if error == 0 {
            logs::logs_output::logs_output(
                LOG_LOCATION,
                "File picker cancelled, nothing added.",
                "info",
            );
        } else {
            logs::logs_output::logs_output(
                LOG_LOCATION,
                &format!("File picker failed with error 0x{:X}.", error),
                "err",
            );
        }
        return Vec::new();
    }

    let paths = parse_multi_selection(&buffer);
    logs::logs_output::logs_output(
        LOG_LOCATION,
        &format!("File picker returned {} path(s).", paths.len()),
        "info",
    );
    paths
}

/// Splits the buffer of a multi selection dialog into absolute paths.
///
/// The API writes either one complete path (`C:\dir\file.exe`) or, when several
/// files were selected, the directory followed by the bare file names
/// (`C:\dir\0a.exe\0b.exe\0\0`). Both forms are recognised here.
#[cfg(windows)]
fn parse_multi_selection(buffer: &[u16]) -> Vec<String> {
    let mut segments: Vec<String> = Vec::new();
    let mut start = 0usize;
    for (index, unit) in buffer.iter().enumerate() {
        if *unit != 0 {
            continue;
        }
        if index == start {
            break; // Empty segment: end of the list.
        }
        segments.push(String::from_utf16_lossy(&buffer[start..index]));
        start = index + 1;
    }

    match segments.len() {
        0 => Vec::new(),
        1 => segments,
        _ => {
            // First segment is the directory, the rest are file names.
            let directory = segments.remove(0);
            segments
                .into_iter()
                .map(|name| format!("{}\\{}", directory.trim_end_matches('\\'), name))
                .collect()
        }
    }
}

/// Opens the folder picker and returns the chosen folder.
///
/// `SHBrowseForFolderW` is used instead of the newer `IFileOpenDialog` on
/// purpose: the COM based dialog needs an OLE apartment that the rest of the
/// application does not use, while this one works with a plain message loop.
#[cfg(windows)]
pub fn pick_folder(owner: *mut c_void, title: &str) -> Option<String> {
    #[repr(C)]
    #[derive(Default)]
    struct BrowseInfoW {
        hwnd_owner: *mut c_void,
        pidl_root: *mut c_void,
        psz_display_name: *mut u16,
        lpsz_title: *const u16,
        ul_flags: u32,
        lpfn: *mut c_void,
        l_param: isize,
        i_image: i32,
    }

    #[link(name = "shell32")]
    unsafe extern "system" {
        /// Shows the folder tree dialog; returns an item id list or null.
        fn SHBrowseForFolderW(info: *mut BrowseInfoW) -> *mut c_void;
        /// Converts such an item id list back into a path.
        fn SHGetPathFromIDListW(pidl: *const c_void, path: *mut u16) -> i32;
    }

    #[link(name = "ole32")]
    unsafe extern "system" {
        /// Frees the item id list allocated by `SHBrowseForFolderW`.
        fn CoTaskMemFree(pointer: *mut c_void);
    }

    // BIF_RETURNONLYFSDIRS (only file system folders) | BIF_EDITBOX (the user
    // may type a path, which is the only way to reach a hidden folder here).
    const FLAGS: u32 = 0x0001 | 0x0010;
    /// `MAX_PATH` plus room for the trailing NUL.
    const BUFFER_LEN: usize = 260;

    let title_wide = wide(title);
    let mut display = vec![0u16; BUFFER_LEN];
    let mut info = BrowseInfoW {
        hwnd_owner: owner,
        psz_display_name: display.as_mut_ptr(),
        lpsz_title: title_wide.as_ptr(),
        ul_flags: FLAGS,
        ..Default::default()
    };

    let pidl = unsafe { SHBrowseForFolderW(&mut info) };
    if pidl.is_null() {
        logs::logs_output::logs_output(
            LOG_LOCATION,
            "Folder picker cancelled, nothing added.",
            "info",
        );
        return None;
    }

    let mut path = vec![0u16; BUFFER_LEN];
    let ok = unsafe { SHGetPathFromIDListW(pidl, path.as_mut_ptr()) };
    unsafe { CoTaskMemFree(pidl) };

    if ok == 0 {
        logs::logs_output::logs_output(
            LOG_LOCATION,
            "Folder picker returned an item that is not a file system path.",
            "err",
        );
        return None;
    }

    let path = String::from_utf16_lossy(&path)
        .trim_end_matches('\0')
        .to_string();
    logs::logs_output::logs_output(
        LOG_LOCATION,
        &format!("Folder picker returned \"{}\".", path),
        "info",
    );
    Some(path)
}

/// Non Windows fallback: the application is Windows only, but the module has to
/// build everywhere the crate is checked.
#[cfg(not(windows))]
pub fn pick_files(_owner: *mut c_void, _title: &str) -> Vec<String> {
    Vec::new()
}

/// Non Windows fallback, see [`pick_files`].
#[cfg(not(windows))]
pub fn pick_folder(_owner: *mut c_void, _title: &str) -> Option<String> {
    None
}
