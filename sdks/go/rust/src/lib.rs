// BoxLite Go SDK - Rust Bridge Layer
//
// This library serves as the underlying core for the Go SDK.
// It maintains a global Runtime on the Rust side and exposes C ABI to Go (CGO).
// NOW REFACTORED to use shared `boxlite-ffi`.

use boxlite_ffi::{
    error::{BoxliteErrorCode, FFIError},
    ops,
    runtime::{BoxHandle, RuntimeHandle},
};
use std::ffi::c_char;
use std::sync::OnceLock;

// Global Runtime Holder
// We store usize to make it Sync (raw pointers are not Sync)
static GLOBAL_RUNTIME: OnceLock<usize> = OnceLock::new();
// Store initialization error message to report on subsequent failures
static GLOBAL_INIT_ERROR: OnceLock<String> = OnceLock::new();

unsafe fn get_runtime(out_err: *mut *mut c_char) -> *mut RuntimeHandle {
    let ptr_val = *GLOBAL_RUNTIME.get_or_init(|| {
        let mut handle: *mut RuntimeHandle = std::ptr::null_mut();
        let mut error = FFIError::default();
        
        let code = ops::create_runtime_impl(
            std::ptr::null(), // home_dir
            std::ptr::null(), // registries_json
            &mut handle,
            &mut error,
        );

        if code != BoxliteErrorCode::Ok {
            // Capture the error message
            let msg = if !error.message.is_null() {
                boxlite_ffi::c_str_to_string(error.message).unwrap_or_else(|_| "Unknown initialization error".to_string())
            } else {
                "Unknown initialization error".to_string()
            };
            let _ = GLOBAL_INIT_ERROR.set(msg);
            
            // Free error resource
            ops::error_free_impl(&mut error);
            0
        } else {
            handle as usize
        }
    });

    if ptr_val == 0 {
        // Runtime is not available, return error if requested
        if !out_err.is_null() {
            let err_msg = GLOBAL_INIT_ERROR.get()
                .map(|s| s.as_str())
                .unwrap_or("Runtime failed to initialize");
            
            let c_msg = std::ffi::CString::new(err_msg).unwrap();
            *out_err = c_msg.into_raw();
        }
        std::ptr::null_mut()
    } else {
        ptr_val as *mut RuntimeHandle
    }
}

// Helper to propagate FFIError to C-style out_err (**char)
unsafe fn propagate_error(mut error: FFIError, out_err: *mut *mut c_char) {
    if !out_err.is_null() {
        *out_err = error.message;
    } else {
        // If caller didn't provide out_err, we must free the message to prevent leak
        ops::error_free_impl(&mut error);
    }
}

// Macro to ensure runtime is available
macro_rules! ensure_runtime {
    ($out_err:expr, $err_ret:expr) => {{
        let rt = get_runtime($out_err);
        if rt.is_null() {
            return $err_ret;
        }
        rt
    }};
}

// ============================================================================
// FFI EXPORTED FUNCTIONS (C ABI)
// ============================================================================

/// Simple ping function to verify the Go-Rust bridge is working.
/// Returns 42 on success.
#[no_mangle]
pub extern "C" fn boxlite_go_ping() -> i32 {
    42
}

/// Free a C string allocated by Rust.
#[no_mangle]
pub unsafe extern "C" fn boxlite_go_free_string(s: *mut c_char) {
    ops::string_free_impl(s);
}

// Create a new box with the given options (JSON).
/// Returns box ID (caller must free) on success, NULL on failure.
#[no_mangle]
pub unsafe extern "C" fn boxlite_go_create_box(
    opts_json: *const c_char,
    name: *const c_char,
    out_err: *mut *mut c_char,
) -> *mut c_char {
    let runtime = ensure_runtime!(out_err, std::ptr::null_mut());

    let mut handle: *mut BoxHandle = std::ptr::null_mut();
    let mut error = FFIError::default();

    let code = ops::create_box_impl(runtime, opts_json, name, &mut handle, &mut error);

    if code == BoxliteErrorCode::Ok {
        let id_str = ops::box_id_impl(handle);
        // We only return the ID, so we free the handle immediately
        ops::box_free_impl(handle);
        id_str
    } else {
        propagate_error(error, out_err);
        std::ptr::null_mut()
    }
}

/// Get a box handle by ID or name.
/// Returns BoxHandle pointer on success, NULL if not found or on error.
#[no_mangle]
pub unsafe extern "C" fn boxlite_go_get_box(
    id_or_name: *const c_char,
    out_err: *mut *mut c_char,
) -> *mut BoxHandle {
    let runtime = ensure_runtime!(out_err, std::ptr::null_mut());

    let mut handle: *mut BoxHandle = std::ptr::null_mut();
    let mut error = FFIError::default();

    let code = ops::get_box_impl(runtime, id_or_name, &mut handle, &mut error);

    if code == BoxliteErrorCode::Ok {
        handle
    } else if code == BoxliteErrorCode::NotFound {
        // Original behavior: return NULL, but NO error message for NotFound
        ops::error_free_impl(&mut error);
        std::ptr::null_mut()
    } else {
        propagate_error(error, out_err);
        std::ptr::null_mut()
    }
}

/// List all boxes as JSON array.
/// Returns 0 on success, -1 on error.
#[no_mangle]
pub unsafe extern "C" fn boxlite_go_list_boxes(
    out_json: *mut *mut c_char,
    out_err: *mut *mut c_char,
) -> i32 {
    let runtime = ensure_runtime!(out_err, -1);

    let mut error = FFIError::default();
    let code = ops::list_boxes_impl(runtime, out_json, &mut error);

    if code == BoxliteErrorCode::Ok {
        0
    } else {
        propagate_error(error, out_err);
        -1
    }
}

/// Remove a box by ID or name.
/// Returns 0 on success, -1 on error.
#[no_mangle]
pub unsafe extern "C" fn boxlite_go_remove_box(
    id_or_name: *const c_char,
    force: bool,
    out_err: *mut *mut c_char,
) -> i32 {
    let runtime = ensure_runtime!(out_err, -1);

    let mut error = FFIError::default();
    let code = ops::remove_impl(runtime, id_or_name, force, &mut error);

    if code == BoxliteErrorCode::Ok {
        0
    } else {
        propagate_error(error, out_err);
        -1
    }
}

/// Start a box.
/// Returns 0 on success, -1 on error.
#[no_mangle]
pub unsafe extern "C" fn boxlite_go_box_start(
    handle: *mut BoxHandle,
    out_err: *mut *mut c_char,
) -> i32 {
    let mut error = FFIError::default();
    // Corrected function name: start_box_impl (was box_start_impl)
    let code = ops::start_box_impl(handle, &mut error);

    if code == BoxliteErrorCode::Ok {
        0
    } else {
        propagate_error(error, out_err);
        -1
    }
}

/// Stop a box.
/// Returns 0 on success, -1 on error.
#[no_mangle]
pub unsafe extern "C" fn boxlite_go_box_stop(
    handle: *mut BoxHandle,
    out_err: *mut *mut c_char,
) -> i32 {
    let mut error = FFIError::default();
    // Corrected function name: stop_box_impl (was box_stop_impl)
    let code = ops::stop_box_impl(handle, &mut error);

    if code == BoxliteErrorCode::Ok {
        0
    } else {
        propagate_error(error, out_err);
        -1
    }
}

/// Get box info as JSON.
/// Returns 0 on success, -1 on error.
#[no_mangle]
pub unsafe extern "C" fn boxlite_go_box_info(
    handle: *mut BoxHandle,
    out_json: *mut *mut c_char,
    out_err: *mut *mut c_char,
) -> i32 {
    let mut error = FFIError::default();
    let code = ops::box_info_impl(handle, out_json, &mut error);

    if code == BoxliteErrorCode::Ok {
        0
    } else {
        propagate_error(error, out_err);
        -1
    }
}

/// Get box ID as string (caller must free).
#[no_mangle]
pub unsafe extern "C" fn boxlite_go_box_id(handle: *mut BoxHandle) -> *mut c_char {
    ops::box_id_impl(handle)
}

/// Free a box handle.
#[no_mangle]
pub unsafe extern "C" fn boxlite_go_box_free(handle: *mut BoxHandle) {
    // Now uses correct implementation
    ops::box_free_impl(handle);
}
