//! Versioned C ABI for native hosts that cannot consume the Rust API directly.
use crate::{
    api::{DashrSession, FrameOutput, GpuOptions, TerminationStatus},
    asset_format::AssetDocument,
    settings::Settings,
};
use anyhow::{Context, Result, ensure};
use std::{
    cell::RefCell,
    ffi::{CStr, CString, c_char, c_void},
    panic::{AssertUnwindSafe, catch_unwind},
    path::PathBuf,
    ptr,
};

pub const DASHR_FFI_ABI_VERSION: u32 = 1;
pub const DASHR_STATUS_NOT_LAUNCHED: u32 = 0;
pub const DASHR_STATUS_HIT: u32 = 1;
pub const DASHR_STATUS_ESCAPED: u32 = 2;
pub const DASHR_STATUS_BUDGET_EXHAUSTED: u32 = 3;
pub const DASHR_STATUS_INVALID_BASIS: u32 = 4;
pub const DASHR_STATUS_DEBUG_FORCED_HIT: u32 = 5;
pub const DASHR_LOG_DEBUG: u32 = 0;
pub const DASHR_LOG_INFO: u32 = 1;
pub const DASHR_LOG_WARNING: u32 = 2;
pub const DASHR_LOG_ERROR: u32 = 3;

/// Nullable host callback invoked synchronously on the thread that registered it.
pub type DashrLogCallback = Option<unsafe extern "C" fn(u32, *const c_char, *mut c_void)>;

#[derive(Clone, Copy)]
struct LogRegistration {
    callback: unsafe extern "C" fn(u32, *const c_char, *mut c_void),
    user_data: *mut c_void,
}

thread_local! {
    static LAST_ERROR: RefCell<CString> = RefCell::new(CString::default());
    static LOG_CALLBACK: RefCell<Option<LogRegistration>> = const { RefCell::new(None) };
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DashrCTrace {
    pub struct_size: u32,
    pub status: u32,
    pub steps: u32,
    pub teleports: u32,
    pub auxiliary: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct DashrCFrameView {
    pub struct_size: u32,
    pub abi_version: u32,
    pub width: u32,
    pub height: u32,
    /// `width * height * 4` floats in RGBA order. Valid until the next handle call.
    pub color_rgba: *const f32,
    /// `width * height * 4` floats: hit U, hit V, ray distance, reverse-Z depth.
    pub hit_uv_distance_depth: *const f32,
    /// `width * height` entries. Valid until the next handle call.
    pub primary: *const DashrCTrace,
    /// `width * height` entries. Valid until the next handle call.
    pub shadow: *const DashrCTrace,
}

impl Default for DashrCFrameView {
    fn default() -> Self {
        Self {
            struct_size: std::mem::size_of::<Self>() as u32,
            abi_version: DASHR_FFI_ABI_VERSION,
            width: 0,
            height: 0,
            color_rgba: ptr::null(),
            hit_uv_distance_depth: ptr::null(),
            primary: ptr::null(),
            shadow: ptr::null(),
        }
    }
}

/// Opaque C handle. Calls that use a handle must be serialized on one host thread.
pub struct DashrFfiSession {
    session: DashrSession,
    frame: Option<FrameOutput>,
    primary: Vec<DashrCTrace>,
    shadow: Vec<DashrCTrace>,
    view: DashrCFrameView,
}

impl DashrFfiSession {
    fn render(&mut self) -> Result<*const DashrCFrameView> {
        self.frame = Some(self.session.render()?);
        let frame = self.frame.as_ref().unwrap();
        self.primary = frame.primary.iter().copied().map(c_trace).collect();
        self.shadow = frame.shadow.iter().copied().map(c_trace).collect();
        self.view = DashrCFrameView {
            struct_size: std::mem::size_of::<DashrCFrameView>() as u32,
            abi_version: DASHR_FFI_ABI_VERSION,
            width: frame.width,
            height: frame.height,
            color_rgba: frame.color.as_ptr().cast(),
            hit_uv_distance_depth: frame.hit_uv_distance_depth.as_ptr().cast(),
            primary: self.primary.as_ptr(),
            shadow: self.shadow.as_ptr(),
        };
        Ok(&self.view)
    }
}

fn c_trace(trace: crate::api::TraceDiagnostic) -> DashrCTrace {
    DashrCTrace {
        struct_size: std::mem::size_of::<DashrCTrace>() as u32,
        status: match trace.status {
            TerminationStatus::NotLaunched => DASHR_STATUS_NOT_LAUNCHED,
            TerminationStatus::Hit => DASHR_STATUS_HIT,
            TerminationStatus::Escaped => DASHR_STATUS_ESCAPED,
            TerminationStatus::BudgetExhausted => DASHR_STATUS_BUDGET_EXHAUSTED,
            TerminationStatus::InvalidBasis => DASHR_STATUS_INVALID_BASIS,
            TerminationStatus::DebugForcedHit => DASHR_STATUS_DEBUG_FORCED_HIT,
        },
        steps: trace.steps,
        teleports: trace.teleports,
        auxiliary: trace.auxiliary,
    }
}

fn set_error(message: &str) {
    let sanitized = message.replace('\0', "\\0");
    let value = CString::new(sanitized).unwrap_or_default();
    LAST_ERROR.with(|slot| *slot.borrow_mut() = value);
}

fn emit_log(level: u32, message: &str) {
    let registration = LOG_CALLBACK.with(|slot| *slot.borrow());
    if let Some(registration) = registration {
        let sanitized = message.replace('\0', "\\0");
        let message = CString::new(sanitized).unwrap_or_default();
        // The registration is copied before invoking host code, so reentrant calls may update it.
        // SAFETY: the host registers a valid C callback and keeps user_data alive for the call.
        unsafe {
            (registration.callback)(level, message.as_ptr(), registration.user_data);
        }
    }
}

fn ffi_result<T: Copy>(fallback: T, call: impl FnOnce() -> Result<T>) -> T {
    match catch_unwind(AssertUnwindSafe(call)) {
        Ok(Ok(value)) => {
            set_error("");
            value
        }
        Ok(Err(error)) => {
            let message = format!("{error:#}");
            set_error(&message);
            emit_log(DASHR_LOG_ERROR, &message);
            fallback
        }
        Err(_) => {
            let message = "DASHR caught a panic at the C ABI boundary";
            set_error(message);
            emit_log(DASHR_LOG_ERROR, message);
            fallback
        }
    }
}

unsafe fn input_string(value: *const c_char, name: &str) -> Result<String> {
    ensure!(!value.is_null(), "{name} pointer is null");
    // SAFETY: the C ABI contract requires a valid NUL-terminated string.
    unsafe { CStr::from_ptr(value) }
        .to_str()
        .with_context(|| format!("{name} is not valid UTF-8"))
        .map(str::to_owned)
}

/// Returns the version of the C struct/function contract, separate from asset and Rust API versions.
#[unsafe(no_mangle)]
pub extern "C" fn dashr_ffi_abi_version() -> u32 {
    DASHR_FFI_ABI_VERSION
}

/// Returns a thread-local error string, valid until the next DASHR FFI call on this thread.
#[unsafe(no_mangle)]
pub extern "C" fn dashr_last_error_message() -> *const c_char {
    LAST_ERROR.with(|slot| slot.borrow().as_ptr())
}

/// Registers or clears a per-thread host log callback.
///
/// The callback is invoked synchronously on this thread. `message` is a temporary NUL-terminated
/// UTF-8 string valid only for the callback duration. The host owns `user_data` and must keep it
/// valid until it clears the registration. Passing a null callback unregisters the current one.
#[unsafe(no_mangle)]
pub extern "C" fn dashr_set_log_callback(
    callback: DashrLogCallback,
    user_data: *mut c_void,
) -> i32 {
    ffi_result(-1, || {
        LOG_CALLBACK.with(|slot| {
            *slot.borrow_mut() = callback.map(|callback| LogRegistration {
                callback,
                user_data,
            });
        });
        Ok(0)
    })
}

/// Creates a headless session from optional settings/asset JSON and an asset root path.
///
/// A null settings string selects `Settings::default`. A null asset string selects the procedural
/// mesh in settings; otherwise the string must contain a valid versioned `AssetDocument`. The
/// asset root must be a valid NUL-terminated UTF-8 string. The returned handle must be destroyed
/// exactly once.
///
/// # Safety
/// Each non-null input pointer must reference a readable NUL-terminated string for the duration
/// of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dashr_session_create(
    settings_json: *const c_char,
    asset_json: *const c_char,
    asset_root: *const c_char,
) -> *mut DashrFfiSession {
    ffi_result(ptr::null_mut(), || {
        let settings = if settings_json.is_null() {
            Settings::default()
        } else {
            // SAFETY: forwarded C string follows this function's documented pointer contract.
            let json = unsafe { input_string(settings_json, "settings_json")? };
            serde_json::from_str(&json).context("decode settings JSON")?
        };
        // SAFETY: forwarded C strings follow this function's documented pointer contract.
        let asset_root = unsafe { input_string(asset_root, "asset_root")? };
        let root = PathBuf::from(asset_root);
        let session = if asset_json.is_null() {
            pollster::block_on(DashrSession::headless(
                settings,
                root,
                GpuOptions::default(),
            ))?
        } else {
            // SAFETY: forwarded C string follows this function's documented pointer contract.
            let asset_json = unsafe { input_string(asset_json, "asset_json")? };
            let asset = AssetDocument::from_json(&asset_json)?;
            pollster::block_on(DashrSession::from_asset(
                settings,
                asset,
                root,
                GpuOptions::default(),
            ))?
        };
        let handle = Box::into_raw(Box::new(DashrFfiSession {
            session,
            frame: None,
            primary: Vec::new(),
            shadow: Vec::new(),
            view: DashrCFrameView::default(),
        }));
        emit_log(DASHR_LOG_INFO, "session created");
        Ok(handle)
    })
}

/// Updates the four-bone reference animation. Returns zero on success and minus one on error.
///
/// # Safety
/// `handle` must be a live handle returned by `dashr_session_create`; calls using it must be
/// serialized on the creating thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dashr_session_update_pose(
    handle: *mut DashrFfiSession,
    time: f32,
    animation_amount: f32,
) -> i32 {
    ffi_result(-1, || {
        ensure!(!handle.is_null(), "session handle is null");
        // SAFETY: the caller owns a live handle returned by `dashr_session_create` and serializes calls.
        let session = unsafe { &mut *handle };
        session.session.update_pose(time, animation_amount)?;
        session.frame = None;
        session.primary.clear();
        session.shadow.clear();
        session.view = DashrCFrameView::default();
        emit_log(DASHR_LOG_DEBUG, "pose updated");
        Ok(0)
    })
}

/// Renders and returns a borrowed frame view, or null on error.
///
/// All pointers in the returned view remain valid until the next update, render, or destroy call
/// on this handle. The caller must serialize handle access.
///
/// # Safety
/// `handle` must be a live handle returned by `dashr_session_create`; calls using it must be
/// serialized on the creating thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dashr_session_render(
    handle: *mut DashrFfiSession,
) -> *const DashrCFrameView {
    ffi_result(ptr::null(), || {
        ensure!(!handle.is_null(), "session handle is null");
        // SAFETY: the caller owns a live handle returned by `dashr_session_create` and serializes calls.
        unsafe { &mut *handle }.render()
    })
}

/// Destroys a session handle. Passing null is an error and does not modify memory.
///
/// # Safety
/// A non-null `handle` must be a live handle returned by `dashr_session_create`, and it must not
/// have been destroyed previously.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dashr_session_destroy(handle: *mut DashrFfiSession) {
    ffi_result((), || {
        ensure!(!handle.is_null(), "session handle is null");
        emit_log(DASHR_LOG_INFO, "session destroyed");
        // SAFETY: the caller must destroy each live handle exactly once.
        drop(unsafe { Box::from_raw(handle) });
        Ok(())
    });
}

/// Maps a public status code to the stable numeric C status value, or `u32::MAX` if unknown.
#[unsafe(no_mangle)]
pub extern "C" fn dashr_status_code(status: u32) -> u32 {
    match status {
        DASHR_STATUS_NOT_LAUNCHED..=DASHR_STATUS_DEBUG_FORCED_HIT => status,
        _ => u32::MAX,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    thread_local! {
        static LOG_EVENTS: RefCell<Vec<(u32, String)>> = const { RefCell::new(Vec::new()) };
    }

    unsafe extern "C" fn record_log(
        level: u32,
        message: *const c_char,
        _user_data: *mut std::ffi::c_void,
    ) {
        // SAFETY: the FFI logger passes a temporary, NUL-terminated message for the call duration.
        let message = unsafe { CStr::from_ptr(message) }
            .to_string_lossy()
            .into_owned();
        LOG_EVENTS.with(|events| events.borrow_mut().push((level, message)));
    }

    #[test]
    fn ffi_structs_have_documented_c_layout_sizes() {
        assert_eq!(std::mem::size_of::<DashrCTrace>(), 20);
        assert_eq!(
            std::mem::size_of::<DashrCFrameView>(),
            16 + 4 * std::mem::size_of::<*const ()>()
        );
        assert_eq!(dashr_ffi_abi_version(), 1);
        assert_eq!(dashr_status_code(5), 5);
        assert_eq!(dashr_status_code(6), u32::MAX);
    }

    #[test]
    fn ffi_errors_are_returned_without_crossing_the_boundary() {
        let malformed_settings = CString::new("{").unwrap();
        let root = CString::new(".").unwrap();
        // SAFETY: both strings are valid NUL-terminated pointers and the null asset selects a
        // procedural mesh; malformed settings must fail before any GPU is created.
        let handle = unsafe {
            dashr_session_create(malformed_settings.as_ptr(), ptr::null(), root.as_ptr())
        };
        assert!(handle.is_null());
        // SAFETY: the returned pointer is a valid thread-local C string until the next ABI call.
        let message = unsafe { CStr::from_ptr(dashr_last_error_message()) }
            .to_str()
            .unwrap();
        assert!(message.contains("settings JSON"));
    }

    #[test]
    fn ffi_error_logs_are_delivered_to_the_thread_callback() {
        LOG_EVENTS.with(|events| events.borrow_mut().clear());
        assert_eq!(dashr_set_log_callback(Some(record_log), ptr::null_mut()), 0);

        let result = ffi_result(-1, || Err(anyhow::anyhow!("callback regression error")));

        assert_eq!(result, -1);
        let logs = LOG_EVENTS.with(|events| events.borrow().clone());
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].0, 3);
        assert!(logs[0].1.contains("callback regression error"));
        assert_eq!(dashr_set_log_callback(None, ptr::null_mut()), 0);
    }
}
