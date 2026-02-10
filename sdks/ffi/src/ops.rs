//! Core FFI operations for BoxLite
//!
//! This module contains the internal implementation of FFI operations.
//! These functions are called by the SDK-specific FFI exports.

use futures::StreamExt;
use std::ffi::{CString, c_void};
use std::os::raw::{c_char, c_int};
use std::ptr;

use boxlite::BoxliteError;
use boxlite::litebox::LiteBox;
use boxlite::runtime::BoxliteRuntime;
use boxlite::runtime::options::{BoxOptions, BoxliteOptions};
use boxlite::runtime::types::BoxID;

use crate::error::{BoxliteErrorCode, FFIError, error_to_code, null_pointer_error, write_error};
use crate::json::box_info_to_json;
use crate::runtime::{BoxHandle, RuntimeHandle, create_tokio_runtime};
use crate::string::c_str_to_string;

/// Create a new BoxliteRuntime
///
/// # Safety
/// All pointer parameters must be valid or null
pub unsafe fn create_runtime_impl(
    home_dir: *const c_char,
    registries_json: *const c_char,
    out_runtime: *mut *mut RuntimeHandle,
    out_error: *mut FFIError,
) -> BoxliteErrorCode {
    unsafe {
        if out_runtime.is_null() {
            write_error(out_error, null_pointer_error("out_runtime"));
            return BoxliteErrorCode::InvalidArgument;
        }

        // Create tokio runtime
        let tokio_rt = match create_tokio_runtime() {
            Ok(rt) => rt,
            Err(e) => {
                let err = BoxliteError::Internal(e);
                write_error(out_error, err);
                return BoxliteErrorCode::Internal;
            }
        };

        // Parse options
        let mut options = BoxliteOptions::default();
        if !home_dir.is_null() {
            match c_str_to_string(home_dir) {
                Ok(path) => options.home_dir = path.into(),
                Err(e) => {
                    write_error(out_error, e);
                    return BoxliteErrorCode::InvalidArgument;
                }
            }
        }

        // Parse image registries (JSON array)
        if !registries_json.is_null() {
            match c_str_to_string(registries_json) {
                Ok(json_str) => match serde_json::from_str::<Vec<String>>(&json_str) {
                    Ok(registries) => options.image_registries = registries,
                    Err(e) => {
                        let err = BoxliteError::Internal(format!("Invalid registries JSON: {}", e));
                        write_error(out_error, err);
                        return BoxliteErrorCode::Internal;
                    }
                },
                Err(e) => {
                    write_error(out_error, e);
                    return BoxliteErrorCode::InvalidArgument;
                }
            }
        }

        // Create runtime
        let runtime = match BoxliteRuntime::new(options) {
            Ok(rt) => rt,
            Err(e) => {
                let code = error_to_code(&e);
                write_error(out_error, e);
                return code;
            }
        };

        *out_runtime = Box::into_raw(Box::new(RuntimeHandle { runtime, tokio_rt }));
        BoxliteErrorCode::Ok
    }
}

/// Create a new box
///
/// # Safety
/// All pointer parameters must be valid or null
pub unsafe fn create_box_impl(
    runtime: *mut RuntimeHandle,
    options_json: *const c_char,
    name: *const c_char,
    out_box: *mut *mut BoxHandle,
    out_error: *mut FFIError,
) -> BoxliteErrorCode {
    unsafe {
        if runtime.is_null() {
            write_error(out_error, null_pointer_error("runtime"));
            return BoxliteErrorCode::InvalidArgument;
        }
        if out_box.is_null() {
            write_error(out_error, null_pointer_error("out_box"));
            return BoxliteErrorCode::InvalidArgument;
        }

        let runtime_ref = &mut *runtime;

        // Parse JSON options
        let options_str = match c_str_to_string(options_json) {
            Ok(s) => s,
            Err(e) => {
                write_error(out_error, e);
                return BoxliteErrorCode::InvalidArgument;
            }
        };

        // Parse optional name
        let name_opt = if name.is_null() {
            None
        } else {
            match c_str_to_string(name) {
                Ok(s) => Some(s),
                Err(e) => {
                    write_error(out_error, e);
                    return BoxliteErrorCode::InvalidArgument;
                }
            }
        };

        let options: BoxOptions = match serde_json::from_str(&options_str) {
            Ok(opts) => opts,
            Err(e) => {
                let err = BoxliteError::Internal(format!("Invalid JSON options: {}", e));
                write_error(out_error, err);
                return BoxliteErrorCode::Internal;
            }
        };

        eprintln!(
            "DEBUG: SDK create_box: id={:?}, auto_remove={}",
            name_opt, options.auto_remove
        );

        // Create box
        let result = runtime_ref
            .tokio_rt
            .block_on(runtime_ref.runtime.create(options, name_opt));

        match result {
            Ok(handle) => {
                let box_id = handle.id().clone();
                *out_box = Box::into_raw(Box::new(BoxHandle {
                    handle,
                    box_id,
                    tokio_rt: runtime_ref.tokio_rt.clone(),
                }));
                BoxliteErrorCode::Ok
            }
            Err(e) => {
                let code = error_to_code(&e);
                write_error(out_error, e);
                code
            }
        }
    }
}

/// List all boxes as JSON
///
/// # Safety
/// All pointer parameters must be valid or null
pub unsafe fn list_boxes_impl(
    runtime: *mut RuntimeHandle,
    out_json: *mut *mut c_char,
    out_error: *mut FFIError,
) -> BoxliteErrorCode {
    unsafe {
        if runtime.is_null() {
            write_error(out_error, null_pointer_error("runtime"));
            return BoxliteErrorCode::InvalidArgument;
        }
        if out_json.is_null() {
            write_error(out_error, null_pointer_error("out_json"));
            return BoxliteErrorCode::InvalidArgument;
        }

        let runtime_ref = &*runtime;

        let result = runtime_ref
            .tokio_rt
            .block_on(runtime_ref.runtime.list_info());

        match result {
            Ok(boxes) => {
                let json_array: Vec<serde_json::Value> =
                    boxes.iter().map(box_info_to_json).collect();
                let json_str = match serde_json::to_string(&json_array) {
                    Ok(s) => s,
                    Err(e) => {
                        let err =
                            BoxliteError::Internal(format!("JSON serialization failed: {}", e));
                        write_error(out_error, err);
                        return BoxliteErrorCode::Internal;
                    }
                };

                match CString::new(json_str) {
                    Ok(s) => {
                        *out_json = s.into_raw();
                        BoxliteErrorCode::Ok
                    }
                    Err(e) => {
                        let err =
                            BoxliteError::Internal(format!("CString conversion failed: {}", e));
                        write_error(out_error, err);
                        BoxliteErrorCode::Internal
                    }
                }
            }
            Err(e) => {
                let code = error_to_code(&e);
                write_error(out_error, e);
                code
            }
        }
    }
}

/// Stop a box
///
/// # Safety
/// handle must be a valid pointer
pub unsafe fn stop_box_impl(handle: *mut BoxHandle, out_error: *mut FFIError) -> BoxliteErrorCode {
    unsafe {
        if handle.is_null() {
            write_error(out_error, null_pointer_error("handle"));
            return BoxliteErrorCode::InvalidArgument;
        }

        let handle_ref = &*handle;

        let result = handle_ref.tokio_rt.block_on(handle_ref.handle.stop());
        match result {
            Ok(_) => BoxliteErrorCode::Ok,
            Err(e) => {
                let code = error_to_code(&e);
                write_error(out_error, e);
                code
            }
        }
    }
}

/// Get single box info as JSON
///
/// # Safety
/// All pointer parameters must be valid or null
pub unsafe fn get_info_impl(
    runtime: *mut RuntimeHandle,
    id_or_name: *const c_char,
    out_json: *mut *mut c_char,
    out_error: *mut FFIError,
) -> BoxliteErrorCode {
    unsafe {
        if runtime.is_null() {
            write_error(out_error, null_pointer_error("runtime"));
            return BoxliteErrorCode::InvalidArgument;
        }
        if out_json.is_null() {
            write_error(out_error, null_pointer_error("out_json"));
            return BoxliteErrorCode::InvalidArgument;
        }

        let runtime_ref = &*runtime;

        let id_str = match c_str_to_string(id_or_name) {
            Ok(s) => s,
            Err(e) => {
                write_error(out_error, e);
                return BoxliteErrorCode::InvalidArgument;
            }
        };

        let result = runtime_ref
            .tokio_rt
            .block_on(runtime_ref.runtime.get_info(&id_str));

        match result {
            Ok(Some(info)) => {
                let json_str = match serde_json::to_string(&box_info_to_json(&info)) {
                    Ok(s) => s,
                    Err(e) => {
                        let err =
                            BoxliteError::Internal(format!("JSON serialization failed: {}", e));
                        write_error(out_error, err);
                        return BoxliteErrorCode::Internal;
                    }
                };

                match CString::new(json_str) {
                    Ok(s) => {
                        *out_json = s.into_raw();
                        BoxliteErrorCode::Ok
                    }
                    Err(e) => {
                        let err =
                            BoxliteError::Internal(format!("CString conversion failed: {}", e));
                        write_error(out_error, err);
                        BoxliteErrorCode::Internal
                    }
                }
            }
            Ok(None) => {
                let err = BoxliteError::NotFound(format!("Box not found: {}", id_str));
                write_error(out_error, err);
                BoxliteErrorCode::NotFound
            }
            Err(e) => {
                let code = error_to_code(&e);
                write_error(out_error, e);
                code
            }
        }
    }
}

/// Get box handle for reattaching to an existing box
///
/// # Safety
/// All pointer parameters must be valid or null
pub unsafe fn get_box_impl(
    runtime: *mut RuntimeHandle,
    id_or_name: *const c_char,
    out_handle: *mut *mut BoxHandle,
    out_error: *mut FFIError,
) -> BoxliteErrorCode {
    unsafe {
        if runtime.is_null() {
            write_error(out_error, null_pointer_error("runtime"));
            return BoxliteErrorCode::InvalidArgument;
        }
        if out_handle.is_null() {
            write_error(out_error, null_pointer_error("out_handle"));
            return BoxliteErrorCode::InvalidArgument;
        }

        let runtime_ref = &*runtime;

        let id_str = match c_str_to_string(id_or_name) {
            Ok(s) => s,
            Err(e) => {
                write_error(out_error, e);
                return BoxliteErrorCode::InvalidArgument;
            }
        };

        let result = runtime_ref
            .tokio_rt
            .block_on(runtime_ref.runtime.get(&id_str));

        match result {
            Ok(Some(handle)) => {
                let box_id = handle.id().clone();
                *out_handle = Box::into_raw(Box::new(BoxHandle {
                    handle,
                    box_id,
                    tokio_rt: runtime_ref.tokio_rt.clone(),
                }));
                BoxliteErrorCode::Ok
            }
            Ok(None) => {
                let err = BoxliteError::NotFound(format!("Box not found: {}", id_str));
                write_error(out_error, err);
                BoxliteErrorCode::NotFound
            }
            Err(e) => {
                let code = error_to_code(&e);
                write_error(out_error, e);
                code
            }
        }
    }
}

/// Remove a box
///
/// # Safety
/// All pointer parameters must be valid or null
pub unsafe fn remove_impl(
    runtime: *mut RuntimeHandle,
    id_or_name: *const c_char,
    force: bool,
    out_error: *mut FFIError,
) -> BoxliteErrorCode {
    unsafe {
        if runtime.is_null() {
            write_error(out_error, null_pointer_error("runtime"));
            return BoxliteErrorCode::InvalidArgument;
        }

        let runtime_ref = &*runtime;

        let id_str = match c_str_to_string(id_or_name) {
            Ok(s) => s,
            Err(e) => {
                write_error(out_error, e);
                return BoxliteErrorCode::InvalidArgument;
            }
        };

        let result = runtime_ref
            .tokio_rt
            .block_on(runtime_ref.runtime.remove(&id_str, force));

        match result {
            Ok(_) => BoxliteErrorCode::Ok,
            Err(e) => {
                let code = error_to_code(&e);
                write_error(out_error, e);
                code
            }
        }
    }
}

/// Get runtime metrics as JSON
///
/// # Safety
/// All pointer parameters must be valid or null
pub unsafe fn runtime_metrics_impl(
    runtime: *mut RuntimeHandle,
    out_json: *mut *mut c_char,
    out_error: *mut FFIError,
) -> BoxliteErrorCode {
    unsafe {
        if runtime.is_null() {
            write_error(out_error, null_pointer_error("runtime"));
            return BoxliteErrorCode::InvalidArgument;
        }
        if out_json.is_null() {
            write_error(out_error, null_pointer_error("out_json"));
            return BoxliteErrorCode::InvalidArgument;
        }

        let runtime_ref = &*runtime;
        let metrics = runtime_ref.tokio_rt.block_on(runtime_ref.runtime.metrics());

        let json = serde_json::json!({
            "boxes_created_total": metrics.boxes_created_total(),
            "boxes_failed_total": metrics.boxes_failed_total(),
            "num_running_boxes": metrics.num_running_boxes(),
            "total_commands_executed": metrics.total_commands_executed(),
            "total_exec_errors": metrics.total_exec_errors()
        });

        let json_str = match serde_json::to_string(&json) {
            Ok(s) => s,
            Err(e) => {
                let err = BoxliteError::Internal(format!("JSON serialization failed: {}", e));
                write_error(out_error, err);
                return BoxliteErrorCode::Internal;
            }
        };

        match CString::new(json_str) {
            Ok(s) => {
                *out_json = s.into_raw();
                BoxliteErrorCode::Ok
            }
            Err(e) => {
                let err = BoxliteError::Internal(format!("CString conversion failed: {}", e));
                write_error(out_error, err);
                BoxliteErrorCode::Internal
            }
        }
    }
}

/// Gracefully shutdown all boxes in this runtime
///
/// # Safety
/// All pointer parameters must be valid or null
pub unsafe fn runtime_shutdown_impl(
    runtime: *mut RuntimeHandle,
    timeout: Option<i32>,
    out_error: *mut FFIError,
) -> BoxliteErrorCode {
    unsafe {
        if runtime.is_null() {
            write_error(out_error, null_pointer_error("runtime"));
            return BoxliteErrorCode::InvalidArgument;
        }

        let runtime_ref = &*runtime;

        let result = runtime_ref
            .tokio_rt
            .block_on(runtime_ref.runtime.shutdown(timeout));

        match result {
            Ok(()) => BoxliteErrorCode::Ok,
            Err(e) => {
                let code = error_to_code(&e);
                write_error(out_error, e);
                code
            }
        }
    }
}

pub type OutputCallback = extern "C" fn(*const c_char, c_int, *mut c_void);

/// Execute a command in a box
///
/// # Safety
/// All pointer parameters must be valid or null
pub unsafe fn execute_impl(
    handle: *mut BoxHandle,
    command: *const c_char,
    args_json: *const c_char,
    callback: Option<OutputCallback>,
    user_data: *mut c_void,
    out_exit_code: *mut c_int,
    out_error: *mut FFIError,
) -> BoxliteErrorCode {
    unsafe {
        if handle.is_null() {
            write_error(out_error, null_pointer_error("handle"));
            return BoxliteErrorCode::InvalidArgument;
        }

        if out_exit_code.is_null() {
            write_error(out_error, null_pointer_error("out_exit_code"));
            return BoxliteErrorCode::InvalidArgument;
        }

        let handle_ref = &mut *handle;

        // Parse command
        let cmd_str = match c_str_to_string(command) {
            Ok(s) => s,
            Err(e) => {
                let code = error_to_code(&e);
                write_error(out_error, e);
                return code;
            }
        };

        // Parse args
        let args: Vec<String> = if !args_json.is_null() {
            match c_str_to_string(args_json) {
                Ok(json_str) => match serde_json::from_str(&json_str) {
                    Ok(a) => a,
                    Err(e) => {
                        let err = BoxliteError::Internal(format!("Invalid args JSON: {}", e));
                        write_error(out_error, err);
                        return BoxliteErrorCode::InvalidArgument;
                    }
                },
                Err(e) => {
                    let code = error_to_code(&e);
                    write_error(out_error, e);
                    return code;
                }
            }
        } else {
            vec![]
        };

        let mut cmd = boxlite::BoxCommand::new(cmd_str);
        cmd = cmd.args(args);

        // Execute command using new API
        let result = handle_ref.tokio_rt.block_on(async {
            let mut execution = handle_ref.handle.exec(cmd).await?;

            // Stream output to callback if provided
            if let Some(cb) = callback {
                // Take stdout and stderr
                let mut stdout = execution.stdout();
                let mut stderr = execution.stderr();

                // Read both streams
                loop {
                    tokio::select! {
                        Some(line) = async {
                            match &mut stdout {
                                Some(s) => s.next().await,
                                None => None,
                            }
                        } => {
                            if let Ok(c_text) = CString::new(line) {
                                cb(c_text.as_ptr(), 0, user_data); // 0 = stdout
                            }
                        }
                        Some(line) = async {
                            match &mut stderr {
                                Some(s) => s.next().await,
                                None => None,
                            }
                        } => {
                            if let Ok(c_text) = CString::new(line) {
                                cb(c_text.as_ptr(), 1, user_data); // 1 = stderr
                            }
                        }
                        else => break,
                    }
                }
            }

            // Wait for execution to complete
            let status = execution.wait().await?;
            Ok::<i32, BoxliteError>(status.exit_code)
        });

        match result {
            Ok(exit_code) => {
                *out_exit_code = exit_code;
                BoxliteErrorCode::Ok
            }
            Err(e) => {
                let code = error_to_code(&e);
                write_error(out_error, e);
                code
            }
        }
    }
}

/// Get box info from handle as JSON
///
/// # Safety
/// All pointer parameters must be valid or null
pub unsafe fn box_info_impl(
    handle: *mut BoxHandle,
    out_json: *mut *mut c_char,
    out_error: *mut FFIError,
) -> BoxliteErrorCode {
    unsafe {
        if handle.is_null() {
            write_error(out_error, null_pointer_error("handle"));
            return BoxliteErrorCode::InvalidArgument;
        }
        if out_json.is_null() {
            write_error(out_error, null_pointer_error("out_json"));
            return BoxliteErrorCode::InvalidArgument;
        }

        let handle_ref = &*handle;
        let info = handle_ref.handle.info();

        let json_str = match serde_json::to_string(&box_info_to_json(&info)) {
            Ok(s) => s,
            Err(e) => {
                let err = BoxliteError::Internal(format!("JSON serialization failed: {}", e));
                write_error(out_error, err);
                return BoxliteErrorCode::Internal;
            }
        };

        match CString::new(json_str) {
            Ok(s) => {
                *out_json = s.into_raw();
                BoxliteErrorCode::Ok
            }
            Err(e) => {
                let err = BoxliteError::Internal(format!("CString conversion failed: {}", e));
                write_error(out_error, err);
                BoxliteErrorCode::Internal
            }
        }
    }
}

/// Get box metrics from handle as JSON
///
/// # Safety
/// All pointer parameters must be valid or null
pub unsafe fn box_metrics_impl(
    handle: *mut BoxHandle,
    out_json: *mut *mut c_char,
    out_error: *mut FFIError,
) -> BoxliteErrorCode {
    unsafe {
        if handle.is_null() {
            write_error(out_error, null_pointer_error("handle"));
            return BoxliteErrorCode::InvalidArgument;
        }
        if out_json.is_null() {
            write_error(out_error, null_pointer_error("out_json"));
            return BoxliteErrorCode::InvalidArgument;
        }

        let handle_ref = &*handle;

        let result = handle_ref.tokio_rt.block_on(handle_ref.handle.metrics());

        match result {
            Ok(metrics) => {
                let json = serde_json::json!({
                    "cpu_percent": metrics.cpu_percent,
                    "memory_bytes": metrics.memory_bytes,
                    "commands_executed_total": metrics.commands_executed_total,
                    "exec_errors_total": metrics.exec_errors_total,
                    "bytes_sent_total": metrics.bytes_sent_total,
                    "bytes_received_total": metrics.bytes_received_total,
                    "total_create_duration_ms": metrics.total_create_duration_ms,
                    "guest_boot_duration_ms": metrics.guest_boot_duration_ms,
                    "network_bytes_sent": metrics.network_bytes_sent,
                    "network_bytes_received": metrics.network_bytes_received,
                    "network_tcp_connections": metrics.network_tcp_connections,
                    "network_tcp_errors": metrics.network_tcp_errors
                });

                let json_str = match serde_json::to_string(&json) {
                    Ok(s) => s,
                    Err(e) => {
                        let err =
                            BoxliteError::Internal(format!("JSON serialization failed: {}", e));
                        write_error(out_error, err);
                        return BoxliteErrorCode::Internal;
                    }
                };

                match CString::new(json_str) {
                    Ok(s) => {
                        *out_json = s.into_raw();
                        BoxliteErrorCode::Ok
                    }
                    Err(e) => {
                        let err =
                            BoxliteError::Internal(format!("CString conversion failed: {}", e));
                        write_error(out_error, err);
                        BoxliteErrorCode::Internal
                    }
                }
            }
            Err(e) => {
                let code = error_to_code(&e);
                write_error(out_error, e);
                code
            }
        }
    }
}

/// Free a runtime instance
///
/// # Safety
/// runtime must be null or a valid pointer to RuntimeHandle
pub unsafe fn runtime_free_impl(runtime: *mut RuntimeHandle) {
    if !runtime.is_null() {
        unsafe {
            drop(Box::from_raw(runtime));
        }
    }
}

/// Free a string allocated by BoxLite
///
/// # Safety
/// str must be null or a valid pointer to c_char allocated by CString
pub unsafe fn string_free_impl(str: *mut c_char) {
    if !str.is_null() {
        unsafe {
            drop(CString::from_raw(str));
        }
    }
}

/// Free error struct
///
/// # Safety
/// error must be null or a valid pointer to FFIError
pub unsafe fn error_free_impl(error: *mut FFIError) {
    if !error.is_null() {
        unsafe {
            let err = &mut *error;
            if !err.message.is_null() {
                drop(CString::from_raw(err.message));
                err.message = ptr::null_mut();
            }
            err.code = BoxliteErrorCode::Ok;
        }
    }
}

/// Get BoxLite version string
///
/// # Returns
/// Static string containing the version (e.g., "0.1.0")
pub extern "C" fn version_impl() -> *const c_char {
    // Static string, safe to return pointer
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr() as *const c_char
}

/// Create and start a box runner
///
/// # Safety
/// All pointers must be valid
pub unsafe fn runner_new_impl(
    image: *const c_char,
    cpus: c_int,
    memory_mib: c_int,
    out_runner: *mut *mut crate::runner::BoxRunner,
    out_error: *mut FFIError,
) -> BoxliteErrorCode {
    unsafe {
        if image.is_null() {
            write_error(out_error, null_pointer_error("image"));
            return BoxliteErrorCode::InvalidArgument;
        }
        if out_runner.is_null() {
            write_error(out_error, null_pointer_error("out_runner"));
            return BoxliteErrorCode::InvalidArgument;
        }

        let image_str = match c_str_to_string(image) {
            Ok(s) => s,
            Err(e) => {
                write_error(out_error, e);
                return BoxliteErrorCode::InvalidArgument;
            }
        };

        let tokio_rt = match create_tokio_runtime() {
            Ok(rt) => rt,
            Err(e) => {
                let err = BoxliteError::Internal(format!("Failed to create async runtime: {}", e));
                write_error(out_error, err);
                return BoxliteErrorCode::Internal;
            }
        };

        let runtime = match BoxliteRuntime::new(BoxliteOptions::default()) {
            Ok(rt) => rt,
            Err(e) => {
                write_error(out_error, e);
                return BoxliteErrorCode::Internal;
            }
        };

        let options = BoxOptions {
            rootfs: boxlite::runtime::options::RootfsSpec::Image(image_str),
            cpus: if cpus > 0 { Some(cpus as u8) } else { None },
            memory_mib: if memory_mib > 0 {
                Some(memory_mib as u32)
            } else {
                None
            },
            ..Default::default()
        };

        let result = tokio_rt.block_on(async {
            let handle = runtime.create(options, None).await?;
            let box_id = handle.id().clone();
            Ok::<(LiteBox, BoxID), BoxliteError>((handle, box_id))
        });

        match result {
            Ok((handle, box_id)) => {
                let runner = Box::new(crate::runner::BoxRunner::new(
                    runtime, handle, box_id, tokio_rt,
                ));
                *out_runner = Box::into_raw(runner);
                BoxliteErrorCode::Ok
            }
            Err(e) => {
                let code = error_to_code(&e);
                write_error(out_error, e);
                code
            }
        }
    }
}

/// Run a command using the runner
///
/// # Safety
/// All pointers must be valid
pub unsafe fn runner_exec_impl(
    runner: *mut crate::runner::BoxRunner,
    command: *const c_char,
    args: *const *const c_char,
    argc: c_int,
    out_result: *mut *mut crate::runner::ExecResult,
    out_error: *mut FFIError,
) -> BoxliteErrorCode {
    unsafe {
        if runner.is_null() {
            write_error(out_error, null_pointer_error("runner"));
            return BoxliteErrorCode::InvalidArgument;
        }
        if command.is_null() {
            write_error(out_error, null_pointer_error("command"));
            return BoxliteErrorCode::InvalidArgument;
        }
        if out_result.is_null() {
            write_error(out_error, null_pointer_error("out_result"));
            return BoxliteErrorCode::InvalidArgument;
        }

        let runner_ref = &mut *runner;

        let cmd_str = match c_str_to_string(command) {
            Ok(s) => s,
            Err(e) => {
                write_error(out_error, e);
                return BoxliteErrorCode::InvalidArgument;
            }
        };

        let mut arg_vec = Vec::new();
        if !args.is_null() {
            for i in 0..argc {
                let arg_ptr = *args.offset(i as isize);
                if arg_ptr.is_null() {
                    break;
                }
                match c_str_to_string(arg_ptr) {
                    Ok(s) => arg_vec.push(s),
                    Err(e) => {
                        write_error(out_error, e);
                        return BoxliteErrorCode::InvalidArgument;
                    }
                }
            }
        }

        let handle = match &runner_ref.handle {
            Some(h) => h,
            None => {
                write_error(
                    out_error,
                    BoxliteError::InvalidState("Box not initialized".to_string()),
                );
                return BoxliteErrorCode::InvalidState;
            }
        };

        let result = runner_ref.tokio_rt.block_on(async {
            let mut cmd = boxlite::BoxCommand::new(cmd_str);
            cmd = cmd.args(arg_vec);

            let mut execution = handle.exec(cmd).await?;

            let mut stdout_lines = Vec::new();
            let mut stderr_lines = Vec::new();

            let mut stdout_stream = execution.stdout();
            let mut stderr_stream = execution.stderr();

            loop {
                tokio::select! {
                    Some(line) = async {
                        match &mut stdout_stream {
                            Some(s) => s.next().await,
                            None => None,
                        }
                    } => {
                        stdout_lines.push(line);
                    }
                    Some(line) = async {
                        match &mut stderr_stream {
                            Some(s) => s.next().await,
                            None => None,
                        }
                    } => {
                        stderr_lines.push(line);
                    }
                    else => break,
                }
            }

            let status = execution.wait().await?;

            Ok::<(i32, String, String), BoxliteError>((
                status.exit_code,
                stdout_lines.join("\n"),
                stderr_lines.join("\n"),
            ))
        });

        match result {
            Ok((exit_code, stdout, stderr)) => {
                let stdout_c = match CString::new(stdout) {
                    Ok(s) => s.into_raw(),
                    Err(_) => ptr::null_mut(),
                };
                let stderr_c = match CString::new(stderr) {
                    Ok(s) => s.into_raw(),
                    Err(_) => ptr::null_mut(),
                };

                let exec_result = Box::new(crate::runner::ExecResult {
                    exit_code,
                    stdout_text: stdout_c,
                    stderr_text: stderr_c,
                });
                *out_result = Box::into_raw(exec_result);
                BoxliteErrorCode::Ok
            }
            Err(e) => {
                let code = error_to_code(&e);
                write_error(out_error, e);
                code
            }
        }
    }
}

/// Free execution result
///
/// # Safety
/// result must be null or valid pointer
pub unsafe fn result_free_impl(result: *mut crate::runner::ExecResult) {
    if !result.is_null() {
        unsafe {
            let result_box = Box::from_raw(result);
            if !result_box.stdout_text.is_null() {
                drop(CString::from_raw(result_box.stdout_text));
            }
            if !result_box.stderr_text.is_null() {
                drop(CString::from_raw(result_box.stderr_text));
            }
        }
    }
}

/// Free runner (auto-cleanup)
///
/// # Safety
/// runner must be null or valid pointer
pub unsafe fn runner_free_impl(runner: *mut crate::runner::BoxRunner) {
    if !runner.is_null() {
        unsafe {
            let mut runner_box = Box::from_raw(runner);

            if let Some(handle) = runner_box.handle.take() {
                let _ = runner_box.tokio_rt.block_on(handle.stop());
            }

            if let Some(box_id) = runner_box.box_id.take() {
                let _ = runner_box
                    .tokio_rt
                    .block_on(runner_box.runtime.remove(box_id.as_ref(), true));
            }

            drop(runner_box);
        }
    }
}

/// Start or restart a stopped box
///
/// # Safety
/// handle must be valid or null
pub unsafe fn start_box_impl(handle: *mut BoxHandle, out_error: *mut FFIError) -> BoxliteErrorCode {
    unsafe {
        if handle.is_null() {
            write_error(out_error, null_pointer_error("handle"));
            return BoxliteErrorCode::InvalidArgument;
        }

        let handle_ref = &*handle;

        match handle_ref.tokio_rt.block_on(handle_ref.handle.start()) {
            Ok(_) => BoxliteErrorCode::Ok,
            Err(e) => {
                let code = error_to_code(&e);
                write_error(out_error, e);
                code
            }
        }
    }
}

/// Get box ID string from handle
///
/// # Safety
/// handle must be valid or null
pub unsafe fn box_id_impl(handle: *mut BoxHandle) -> *mut c_char {
    unsafe {
        if handle.is_null() {
            return ptr::null_mut();
        }

        let handle_ref = &*handle;
        let id_str = handle_ref.handle.id().to_string();

        match CString::new(id_str) {
            Ok(s) => s.into_raw(),
            Err(_) => ptr::null_mut(),
        }
    }
}

/// Free a box handle
///
/// # Safety
/// handle must be null or a valid pointer to BoxHandle
pub unsafe fn box_free_impl(handle: *mut BoxHandle) {
    if !handle.is_null() {
        unsafe {
            drop(Box::from_raw(handle));
        }
    }
}
