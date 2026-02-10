//! C FFI bindings for BoxLite
//!
//! This module provides a C-compatible API for integrating BoxLite into C/C++ applications.
//! The API uses JSON for complex types to avoid ABI compatibility issues.
//!
//! # Safety
//!
//! All functions in this module are unsafe because they:
//! - Dereference raw pointers passed from C
//! - Require the caller to ensure pointer validity and proper cleanup
//! - May write to caller-provided output pointers

#![allow(unsafe_op_in_unsafe_fn)]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::doc_overindented_list_items)]

use std::os::raw::{c_char, c_int, c_void};

// Import internal FFI types from shared layer
use boxlite_ffi::error::{BoxliteErrorCode, FFIError};
use boxlite_ffi::runner::{BoxRunner, ExecResult};
use boxlite_ffi::runtime::{BoxHandle, RuntimeHandle};

// Define C-compatible type aliases for the C header
pub type CBoxliteRuntime = RuntimeHandle;
pub type CBoxHandle = BoxHandle;
pub type CBoxliteSimple = BoxRunner;
pub type CBoxliteError = FFIError;
pub type CBoxliteExecResult = ExecResult;

// ============================================================================
// Public API Functions
// ============================================================================

/// Get BoxLite version string
#[unsafe(no_mangle)]
pub extern "C" fn boxlite_version() -> *const c_char {
    boxlite_ffi::ops::version_impl()
}

/// Create a new BoxLite runtime
#[unsafe(no_mangle)]
pub unsafe extern "C" fn boxlite_runtime_new(
    home_dir: *const c_char,
    registries_json: *const c_char,
    out_runtime: *mut *mut CBoxliteRuntime,
    out_error: *mut CBoxliteError,
) -> BoxliteErrorCode {
    boxlite_ffi::ops::create_runtime_impl(home_dir, registries_json, out_runtime, out_error)
}

/// Create a new box with the given options (JSON)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn boxlite_create_box(
    runtime: *mut CBoxliteRuntime,
    options_json: *const c_char,
    out_box: *mut *mut CBoxHandle,
    out_error: *mut CBoxliteError,
) -> BoxliteErrorCode {
    boxlite_ffi::ops::create_box_impl(runtime, options_json, std::ptr::null(), out_box, out_error)
}

/// Execute a command in a box
#[unsafe(no_mangle)]
pub unsafe extern "C" fn boxlite_execute(
    handle: *mut CBoxHandle,
    command: *const c_char,
    args_json: *const c_char,
    callback: Option<extern "C" fn(*const c_char, c_int, *mut c_void)>,
    user_data: *mut c_void,
    out_exit_code: *mut c_int,
    out_error: *mut CBoxliteError,
) -> BoxliteErrorCode {
    boxlite_ffi::ops::execute_impl(
        handle,
        command,
        args_json,
        callback,
        user_data,
        out_exit_code,
        out_error,
    )
}

/// Stop a box
#[unsafe(no_mangle)]
pub unsafe extern "C" fn boxlite_stop_box(
    handle: *mut CBoxHandle,
    out_error: *mut CBoxliteError,
) -> BoxliteErrorCode {
    boxlite_ffi::ops::stop_box_impl(handle, out_error)
}

/// List all boxes as JSON
#[unsafe(no_mangle)]
pub unsafe extern "C" fn boxlite_list_info(
    runtime: *mut CBoxliteRuntime,
    out_json: *mut *mut c_char,
    out_error: *mut CBoxliteError,
) -> BoxliteErrorCode {
    boxlite_ffi::ops::list_boxes_impl(runtime, out_json, out_error)
}

/// Get single box info as JSON
#[unsafe(no_mangle)]
pub unsafe extern "C" fn boxlite_get_info(
    runtime: *mut CBoxliteRuntime,
    id_or_name: *const c_char,
    out_json: *mut *mut c_char,
    out_error: *mut CBoxliteError,
) -> BoxliteErrorCode {
    boxlite_ffi::ops::get_info_impl(runtime, id_or_name, out_json, out_error)
}

/// Get box handle for reattaching to an existing box
#[unsafe(no_mangle)]
pub unsafe extern "C" fn boxlite_get(
    runtime: *mut CBoxliteRuntime,
    id_or_name: *const c_char,
    out_handle: *mut *mut CBoxHandle,
    out_error: *mut CBoxliteError,
) -> BoxliteErrorCode {
    boxlite_ffi::ops::get_box_impl(runtime, id_or_name, out_handle, out_error)
}

/// Remove a box
#[unsafe(no_mangle)]
pub unsafe extern "C" fn boxlite_remove(
    runtime: *mut CBoxliteRuntime,
    id_or_name: *const c_char,
    force: c_int,
    out_error: *mut CBoxliteError,
) -> BoxliteErrorCode {
    boxlite_ffi::ops::remove_impl(runtime, id_or_name, force != 0, out_error)
}

/// Get runtime metrics as JSON
#[unsafe(no_mangle)]
pub unsafe extern "C" fn boxlite_runtime_metrics(
    runtime: *mut CBoxliteRuntime,
    out_json: *mut *mut c_char,
    out_error: *mut CBoxliteError,
) -> BoxliteErrorCode {
    boxlite_ffi::ops::runtime_metrics_impl(runtime, out_json, out_error)
}

/// Gracefully shutdown all boxes in this runtime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn boxlite_runtime_shutdown(
    runtime: *mut CBoxliteRuntime,
    timeout: c_int,
    out_error: *mut CBoxliteError,
) -> BoxliteErrorCode {
    // C API: 0 = default (maps to Rust None), positive = timeout, -1 = infinite
    let timeout_opt = if timeout == 0 { None } else { Some(timeout) };
    boxlite_ffi::ops::runtime_shutdown_impl(runtime, timeout_opt, out_error)
}

/// Get box info from handle as JSON
#[unsafe(no_mangle)]
pub unsafe extern "C" fn boxlite_box_info(
    handle: *mut CBoxHandle,
    out_json: *mut *mut c_char,
    out_error: *mut CBoxliteError,
) -> BoxliteErrorCode {
    boxlite_ffi::ops::box_info_impl(handle, out_json, out_error)
}

/// Get box metrics from handle as JSON
#[unsafe(no_mangle)]
pub unsafe extern "C" fn boxlite_box_metrics(
    handle: *mut CBoxHandle,
    out_json: *mut *mut c_char,
    out_error: *mut CBoxliteError,
) -> BoxliteErrorCode {
    boxlite_ffi::ops::box_metrics_impl(handle, out_json, out_error)
}

/// Start or restart a stopped box
#[unsafe(no_mangle)]
pub unsafe extern "C" fn boxlite_start_box(
    handle: *mut CBoxHandle,
    out_error: *mut CBoxliteError,
) -> BoxliteErrorCode {
    boxlite_ffi::ops::start_box_impl(handle, out_error)
}

/// Get box ID string from handle
#[unsafe(no_mangle)]
pub unsafe extern "C" fn boxlite_box_id(handle: *mut CBoxHandle) -> *mut c_char {
    boxlite_ffi::ops::box_id_impl(handle)
}

// ============================================================================
// Runner API (formerly Simple API)
// ============================================================================

/// Create and start a box using simple API
#[unsafe(no_mangle)]
pub unsafe extern "C" fn boxlite_simple_new(
    image: *const c_char,
    cpus: c_int,
    memory_mib: c_int,
    out_box: *mut *mut CBoxliteSimple,
    out_error: *mut CBoxliteError,
) -> BoxliteErrorCode {
    boxlite_ffi::ops::runner_new_impl(image, cpus, memory_mib, out_box, out_error)
}

/// Run a command and get buffered result
#[unsafe(no_mangle)]
pub unsafe extern "C" fn boxlite_simple_run(
    simple_box: *mut CBoxliteSimple,
    command: *const c_char,
    args: *const *const c_char,
    argc: c_int,
    out_result: *mut *mut CBoxliteExecResult,
    out_error: *mut CBoxliteError,
) -> BoxliteErrorCode {
    boxlite_ffi::ops::runner_exec_impl(simple_box, command, args, argc, out_result, out_error)
}

// ============================================================================
// Memory Management
// ============================================================================

/// Free execution result
#[unsafe(no_mangle)]
pub unsafe extern "C" fn boxlite_result_free(result: *mut CBoxliteExecResult) {
    boxlite_ffi::ops::result_free_impl(result);
}

/// Free simple box (auto-cleanup)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn boxlite_simple_free(simple_box: *mut CBoxliteSimple) {
    boxlite_ffi::ops::runner_free_impl(simple_box);
}

/// Free a box handle
#[unsafe(no_mangle)]
pub unsafe extern "C" fn boxlite_box_free(handle: *mut CBoxHandle) {
    boxlite_ffi::ops::box_free_impl(handle);
}

/// Free a runtime instance
#[unsafe(no_mangle)]
pub unsafe extern "C" fn boxlite_runtime_free(runtime: *mut CBoxliteRuntime) {
    boxlite_ffi::ops::runtime_free_impl(runtime);
}

/// Free a string allocated by BoxLite
#[unsafe(no_mangle)]
pub unsafe extern "C" fn boxlite_free_string(str: *mut c_char) {
    boxlite_ffi::ops::string_free_impl(str);
}

/// Free error struct
#[unsafe(no_mangle)]
pub unsafe extern "C" fn boxlite_error_free(error: *mut CBoxliteError) {
    boxlite_ffi::ops::error_free_impl(error);
}
