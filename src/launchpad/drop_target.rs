//! Accepts files and folders that are dragged onto the window from Explorer.
//!
//! # Why this is not a Slint feature
//!
//! Slint 1.17 has no drag and drop API: its winit backend installs an OLE drop
//! target at window creation and then ignores the events it produces. The only
//! way to receive a drop is therefore to replace that target with our own.
//!
//! # What happens on a drop
//!
//! Explorer talks to the registered `IDropTarget` through COM. The incoming
//! `IDataObject` carries a `CF_HDROP` clipboard format, which is a list of
//! full paths, and that list is exactly what the launchpad needs.
//!
//! The `Drop` callback runs inside the OLE drag loop, i.e. in the middle of a
//! nested message loop while the UI is frozen. Writing to Slint from there would
//! be dangerous, so the paths are queued and handed to the event loop with
//! [`slint::invoke_from_event_loop`]; the closure registered by
//! [`install`] then runs at a point where the UI is idle.
//!
//! Everything here is Windows only; the non Windows builds get no-op stubs so
//! the crate keeps compiling.

use std::cell::RefCell;
use std::ffi::c_void;

use crate::logs;

/// Log location shared by every line of this module.
const LOG_LOCATION: &str = "launchpad::drop_target";

/// `S_OK`: the universal success code of COM.
const S_OK: i32 = 0;

/// `E_NOINTERFACE`: the interface asked for by `QueryInterface` is unknown.
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;

/// `DROPEFFECT_COPY`: the drag source should show the "copy" cursor.
const DROPEFFECT_COPY: u32 = 1;

/// `CF_HDROP`: the clipboard format that carries a list of file names.
const CF_HDROP: u16 = 15;

/// `DVASPECT_CONTENT`: the normal representation of the data.
const DVASPECT_CONTENT: u32 = 1;

/// `TYMED_HGLOBAL`: the data is a handle to a block of global memory.
const TYMED_HGLOBAL: u32 = 1;

// Paths collected by the most recent drop, waiting for the event loop.
// A plain comment: a doc comment on a macro invocation is not attached to
// anything and the compiler warns about it.
thread_local! {
    static PENDING_PATHS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    /// Paths parsed in `DragEnter`, used by the matching `Drop`.
    static STAGED_PATHS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    /// Filled in by [`install`]; called on the event loop after a drop.
    static DROP_HANDLER: RefCell<Option<Box<dyn Fn()>>> = const { RefCell::new(None) };
}

/// Everything below is the Windows implementation; it is kept in one module so
/// the rest of the file stays portable and the compiler only needs the Win32
/// symbols when this target is Windows.
#[cfg(windows)]
mod win32 {
    use super::*;

/// Callback of `IDropTarget`, see the COM documentation.
#[repr(C)]
struct DropTargetVtbl {
    query_interface: unsafe extern "system" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32,
    add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    drag_enter: unsafe extern "system" fn(*mut c_void, *mut c_void, u32, Point, *mut u32) -> i32,
    drag_over: unsafe extern "system" fn(*mut c_void, u32, Point, *mut u32) -> i32,
    drag_leave: unsafe extern "system" fn(*mut c_void) -> i32,
    drop: unsafe extern "system" fn(*mut c_void, *mut c_void, u32, Point, *mut u32) -> i32,
}

/// The part of the `IDataObject` vtable that is needed here: the reference
/// counting of `IUnknown` and `GetData`.
#[repr(C)]
struct DataObjectVtbl {
    query_interface: unsafe extern "system" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32,
    add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    get_data: unsafe extern "system" fn(*mut c_void, *const FormatEtc, *mut StgMedium) -> i32,
}

/// `FORMATETC`: describes which representation of the data is wanted.
#[repr(C)]
struct FormatEtc {
    cf_format: u16,
    target_device: *mut c_void,
    aspect: u32,
    index: i32,
    tymed: u32,
}

/// `STGMEDIUM`: the representation that came back. The union of the original
/// structure is a single handle here, which is the `TYMED_HGLOBAL` member.
#[repr(C)]
struct StgMedium {
    tymed: u32,
    handle: *mut c_void,
    release: *mut c_void,
}

/// `POINTL`: a point in screen coordinates, as the drop target receives it.
#[repr(C)]
struct Point {
    x: i32,
    y: i32,
}

/// `GUID`, needed to answer `QueryInterface`.
#[repr(C)]
struct Guid {
    data1: u32,
    data2: u16,
    data3: u16,
    data4: [u8; 8],
}

/// `IID_IDropTarget` = {00000122-0000-0000-C000-000000000046}.
const IID_IDROP_TARGET: Guid = Guid {
    data1: 0x0000_0122,
    data2: 0x0000,
    data3: 0x0000,
    data4: [0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46],
};

/// `IID_IUnknown` = {00000000-0000-0000-C000-000000000046}.
const IID_IUNKNOWN: Guid = Guid {
    data1: 0x0000_0000,
    data2: 0x0000,
    data3: 0x0000,
    data4: [0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46],
};

/// The object handed to COM. It only carries the vtable pointer and a reference
/// count; the paths live in the thread local queues above.
#[repr(C)]
struct DropTarget {
    vtbl: *const DropTargetVtbl,
    ref_count: std::sync::atomic::AtomicU32,
}

#[cfg(windows)]
#[link(name = "ole32")]
unsafe extern "system" {
    /// Initialises COM for the calling thread; `winit` already did this, the
    /// call is repeated here so the module does not depend on that detail.
    fn OleInitialize(reserved: *mut c_void) -> i32;
    /// Replaces the drop target of a window.
    fn RegisterDragDrop(hwnd: *mut c_void, target: *mut c_void) -> i32;
    /// Removes the drop target of a window.
    fn RevokeDragDrop(hwnd: *mut c_void) -> i32;
    /// Frees a `STGMEDIUM` returned by `IDataObject::GetData`.
    fn ReleaseStgMedium(medium: *mut StgMedium);
}

#[cfg(windows)]
#[link(name = "shell32")]
unsafe extern "system" {
    /// Number of paths in an `HDROP` (when the index is `0xFFFFFFFF`), or the
    /// length of one path (when a buffer is passed).
    fn DragQueryFileW(hdrop: *mut c_void, index: u32, buffer: *mut u16, size: u32) -> u32;
}

/// Reads every path out of a `CF_HDROP` data object.
///
/// The data object is only valid for the duration of the drag callback, which is
/// why the paths are copied out immediately.
#[cfg(windows)]
unsafe fn paths_from_data_object(data_object: *mut c_void) -> Vec<String> {
    if data_object.is_null() {
        return Vec::new();
    }

    let vtbl = unsafe { *(data_object as *mut *const DataObjectVtbl) };

    // Ask for the file list: content, first item, global memory.
    let format = FormatEtc {
        cf_format: CF_HDROP,
        target_device: std::ptr::null_mut(),
        aspect: DVASPECT_CONTENT,
        index: -1,
        tymed: TYMED_HGLOBAL,
    };
    let mut medium = StgMedium {
        tymed: 0,
        handle: std::ptr::null_mut(),
        release: std::ptr::null_mut(),
    };

    let result = unsafe { ((*vtbl).get_data)(data_object, &format, &mut medium) };
    if result != S_OK {
        logs::logs_output::logs_output(
            LOG_LOCATION,
            &format!(
                "The dropped data is not a file list (GetData/CF_HDROP answered 0x{:08X}).",
                result
            ),
            "warn",
        );
        return Vec::new();
    }

    let hdrop = medium.handle;
    let mut paths = Vec::new();
    let count = unsafe { DragQueryFileW(hdrop, u32::MAX, std::ptr::null_mut(), 0) };
    for index in 0..count {
        let length = unsafe { DragQueryFileW(hdrop, index, std::ptr::null_mut(), 0) };
        if length == 0 {
            continue;
        }
        let mut buffer = vec![0u16; length as usize + 1];
        let written =
            unsafe { DragQueryFileW(hdrop, index, buffer.as_mut_ptr(), buffer.len() as u32) };
        if written == 0 {
            continue;
        }
        buffer.truncate(written as usize);
        paths.push(String::from_utf16_lossy(&buffer));
    }

    unsafe { ReleaseStgMedium(&mut medium) };
    paths
}

/// `QueryInterface` of `IDropTarget`.
unsafe extern "system" fn query_interface(
    this: *mut c_void,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if riid.is_null() || out.is_null() {
        return E_NOINTERFACE;
    }

    // Compare the two interfaces this object implements, byte by byte.
    let wanted = unsafe { &*riid };
    let wanted_bytes = unsafe {
        std::slice::from_raw_parts(wanted as *const Guid as *const u8, std::mem::size_of::<Guid>())
    };
    let matches = [&IID_IDROP_TARGET, &IID_IUNKNOWN].iter().any(|known| {
        let known_bytes = unsafe {
            std::slice::from_raw_parts(
                *known as *const Guid as *const u8,
                std::mem::size_of::<Guid>(),
            )
        };
        known_bytes == wanted_bytes
    });

    if matches {
        unsafe { *out = this };
        let vtbl = unsafe { (*(this as *const DropTarget)).vtbl };
        unsafe { ((*vtbl).add_ref)(this) };
        S_OK
    } else {
        unsafe { *out = std::ptr::null_mut() };
        E_NOINTERFACE
    }
}

/// `AddRef` of `IDropTarget`.
unsafe extern "system" fn add_ref(this: *mut c_void) -> u32 {
    let target = unsafe { &*(this as *const DropTarget) };
    target
        .ref_count
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        + 1
}

/// `Release` of `IDropTarget`. The object lives as long as the window, so the
/// memory is intentionally not freed here.
unsafe extern "system" fn release(this: *mut c_void) -> u32 {
    let target = unsafe { &*(this as *const DropTarget) };
    target
        .ref_count
        .fetch_sub(1, std::sync::atomic::Ordering::Relaxed)
        .saturating_sub(1)
}

/// `DragEnter`: parses the file list up front and decides whether the drop is
/// acceptable.
unsafe extern "system" fn drag_enter(
    _this: *mut c_void,
    data_object: *mut c_void,
    _key_state: u32,
    _point: Point,
    effect: *mut u32,
) -> i32 {
    let paths = unsafe { paths_from_data_object(data_object) };
    let accepted = !paths.is_empty();
    logs::logs_output::logs_output(
        LOG_LOCATION,
        &format!(
            "Drag entered the window with {} path(s).",
            paths.len()
        ),
        "info",
    );
    STAGED_PATHS.with(|staged| *staged.borrow_mut() = paths);

    if !effect.is_null() {
        unsafe { *effect = if accepted { DROPEFFECT_COPY } else { 0 } };
    }
    S_OK
}

/// `DragOver`: keeps the "copy" cursor while the pointer is over the window.
unsafe extern "system" fn drag_over(
    _this: *mut c_void,
    _key_state: u32,
    _point: Point,
    effect: *mut u32,
) -> i32 {
    let accepted = STAGED_PATHS.with(|staged| !staged.borrow().is_empty());
    if !effect.is_null() {
        unsafe { *effect = if accepted { DROPEFFECT_COPY } else { 0 } };
    }
    S_OK
}

/// `DragLeave`: the drag left the window without being dropped.
unsafe extern "system" fn drag_leave(_this: *mut c_void) -> i32 {
    let dropped = STAGED_PATHS.with(|staged| std::mem::take(&mut *staged.borrow_mut()).len());
    if dropped > 0 {
        logs::logs_output::logs_output(
            LOG_LOCATION,
            &format!("Drag left the window; {} staged path(s) discarded.", dropped),
        "info",
        );
    }
    S_OK
}

/// `Drop`: moves the staged paths into the queue and wakes the event loop.
unsafe extern "system" fn drop(
    _this: *mut c_void,
    data_object: *mut c_void,
    _key_state: u32,
    _point: Point,
    effect: *mut u32,
) -> i32 {
    let mut paths = STAGED_PATHS.with(|staged| std::mem::take(&mut *staged.borrow_mut()));
    if paths.is_empty() {
        // Defensive: some sources hand the data object over only here.
        paths = unsafe { paths_from_data_object(data_object) };
    }

    let accepted = !paths.is_empty();
    if !effect.is_null() {
        unsafe { *effect = if accepted { DROPEFFECT_COPY } else { 0 } };
    }
    if !accepted {
        logs::logs_output::logs_output(
            LOG_LOCATION,
            "A drop happened, but it carried no file or folder.",
            "warn",
        );
        return S_OK;
    }

    logs::logs_output::logs_output(
        LOG_LOCATION,
        &format!("Dropped {} path(s) onto the window.", paths.len()),
        "info",
    );
    PENDING_PATHS.with(|pending| pending.borrow_mut().extend(paths));

    // The OLE drag loop is still running here, so the UI is only touched once
    // the event loop is back in control.
    match slint::invoke_from_event_loop(|| {
        DROP_HANDLER.with(|handler| {
            if let Some(callback) = handler.borrow().as_ref() {
                callback();
            }
        });
    }) {
        Ok(()) => {}
        Err(error) => logs::logs_output::logs_output(
            LOG_LOCATION,
            &format!("The drop could not be handed to the event loop: {error}."),
            "err",
        ),
    }

    S_OK
}

/// The vtable shared by every drop target of the process.
static DROP_TARGET_VTBL: DropTargetVtbl = DropTargetVtbl {
    query_interface,
    add_ref,
    release,
    drag_enter,
    drag_over,
    drag_leave,
    drop,
};


/// Takes the paths collected since the last call.
pub fn take_pending() -> Vec<String> {
    PENDING_PATHS.with(|pending| pending.borrow_mut().drain(..).collect())
}

/// Installs the drop target on the main window.
///
/// `on_drop` is called on the event loop after paths were dropped; it should call
/// [`take_pending`] to read them. The handler is stored thread locally because it
/// is invoked from a closure that the event loop owns.
#[cfg(windows)]
pub fn install<C>(app: &C, on_drop: impl Fn(&C) + 'static)
where
    C: slint::ComponentHandle + 'static,
{
    let handler_app = app.as_weak();
    DROP_HANDLER.with(|handler| {
        *handler.borrow_mut() = Some(Box::new(move || {
            if let Some(app) = handler_app.upgrade() {
                on_drop(&app);
            }
        }));
    });

    let hwnd = crate::windows::window_center::main_window_handle();
    if hwnd.is_null() {
        logs::logs_output::logs_output(
            LOG_LOCATION,
            "Dropping is unavailable: the window handle is unknown.",
            "err",
        );
        return;
    }

    unsafe {
        // Already initialised by the backend; `S_FALSE` simply means "already
        // done", so the result is only logged.
        let ole = OleInitialize(std::ptr::null_mut());
        logs::logs_output::logs_output(
            LOG_LOCATION,
            &format!("OleInitialize answered 0x{ole:08X}."),
            "info",
        );

        // winit installs its own target and drops the events on the floor, so it
        // is replaced instead of chained.
        let revoked = RevokeDragDrop(hwnd);
        logs::logs_output::logs_output(
            LOG_LOCATION,
            &format!("RevokeDragDrop answered 0x{revoked:08X}."),
            "info",
        );
    }

    // The target is leaked on purpose: it has to stay alive for the whole
    // lifetime of the window, and the process ends with the window.
    let target = Box::new(DropTarget {
        vtbl: &DROP_TARGET_VTBL,
        ref_count: std::sync::atomic::AtomicU32::new(1),
    });
    let target = Box::leak(target);

    let result = unsafe {
        RegisterDragDrop(
            hwnd,
            (target as *mut DropTarget).cast::<c_void>(),
        )
    };
    if result == S_OK {
        logs::logs_output::logs_output(
            LOG_LOCATION,
            "Drop target installed: files and folders can be dropped on the window.",
            "info",
        );
    } else {
        logs::logs_output::logs_output(
            LOG_LOCATION,
            &format!("RegisterDragDrop failed with 0x{result:08X}; dropping is unavailable."),
            "err",
        );
    }
}

} // mod win32

/// Public surface of the Windows implementation.
#[cfg(windows)]
pub use win32::{install, take_pending};

/// Non Windows fallback: nothing can ever be dropped.
#[cfg(not(windows))]
pub fn take_pending() -> Vec<String> {
    Vec::new()
}

/// Non Windows fallback: dropping is a Windows feature here.
#[cfg(not(windows))]
pub fn install<C>(_app: &C, _on_drop: impl Fn(&C) + 'static)
where
    C: slint::ComponentHandle + 'static,
{
    logs::logs_output::logs_output(
        LOG_LOCATION,
        "Dropping files is only implemented for Windows.",
        "warn",
    );
}
