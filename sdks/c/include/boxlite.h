#ifndef BOXLITE_H
#define BOXLITE_H

#pragma once

#include <stdarg.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>

// Error codes returned by BoxLite C API functions.
//
// These codes map directly to Rust's BoxliteError variants,
// allowing programmatic error handling in C.
typedef enum BoxliteErrorCode {
  // Operation succeeded
  Ok = 0,
  // Internal error
  Internal = 1,
  // Resource not found
  NotFound = 2,
  // Resource already exists
  AlreadyExists = 3,
  // Invalid state for operation
  InvalidState = 4,
  // Invalid argument provided
  InvalidArgument = 5,
  // Configuration error
  Config = 6,
  // Storage error
  Storage = 7,
  // Image error
  Image = 8,
  // Network error
  Network = 9,
  // Execution error
  Execution = 10,
  // Resource stopped
  Stopped = 11,
  // Engine error
  Engine = 12,
  // Unsupported operation
  Unsupported = 13,
  // Database error
  Database = 14,
  // Portal/communication error
  Portal = 15,
  // RPC error
  Rpc = 16,
  // RPC transport error
  RpcTransport = 17,
  // Metadata error
  Metadata = 18,
  // Unsupported engine error
  UnsupportedEngine = 19,
} BoxliteErrorCode;

// Opaque handle to a running box
typedef struct BoxHandle BoxHandle;

// Opaque handle for Runner API (auto-manages runtime)
typedef struct BoxRunner BoxRunner;

// Opaque handle to a BoxliteRuntime instance with associated Tokio runtime
typedef struct RuntimeHandle RuntimeHandle;

typedef struct RuntimeHandle CBoxliteRuntime;

// Extended error information for C API.
//
// Contains both an error code (for programmatic handling)
// and an optional detailed message (for debugging).
typedef struct FFIError {
  // Error code
  enum BoxliteErrorCode code;
  // Detailed error message (NULL if none, caller must free with boxlite_error_free)
  char *message;
} FFIError;

typedef struct FFIError CBoxliteError;

typedef struct BoxHandle CBoxHandle;

typedef struct BoxRunner CBoxliteSimple;

// Result structure for runner command execution
typedef struct ExecResult {
  int exit_code;
  char *stdout_text;
  char *stderr_text;
} ExecResult;

typedef struct ExecResult CBoxliteExecResult;

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

// Get BoxLite version string
const char *boxlite_version(void);

// Create a new BoxLite runtime
enum BoxliteErrorCode boxlite_runtime_new(const char *home_dir,
                                          const char *registries_json,
                                          CBoxliteRuntime **out_runtime,
                                          CBoxliteError *out_error);

// Create a new box with the given options (JSON)
enum BoxliteErrorCode boxlite_create_box(CBoxliteRuntime *runtime,
                                         const char *options_json,
                                         CBoxHandle **out_box,
                                         CBoxliteError *out_error);

// Execute a command in a box
enum BoxliteErrorCode boxlite_execute(CBoxHandle *handle,
                                      const char *command,
                                      const char *args_json,
                                      void (*callback)(const char*, int, void*),
                                      void *user_data,
                                      int *out_exit_code,
                                      CBoxliteError *out_error);

// Stop a box
enum BoxliteErrorCode boxlite_stop_box(CBoxHandle *handle, CBoxliteError *out_error);

// List all boxes as JSON
enum BoxliteErrorCode boxlite_list_info(CBoxliteRuntime *runtime,
                                        char **out_json,
                                        CBoxliteError *out_error);

// Get single box info as JSON
enum BoxliteErrorCode boxlite_get_info(CBoxliteRuntime *runtime,
                                       const char *id_or_name,
                                       char **out_json,
                                       CBoxliteError *out_error);

// Get box handle for reattaching to an existing box
enum BoxliteErrorCode boxlite_get(CBoxliteRuntime *runtime,
                                  const char *id_or_name,
                                  CBoxHandle **out_handle,
                                  CBoxliteError *out_error);

// Remove a box
enum BoxliteErrorCode boxlite_remove(CBoxliteRuntime *runtime,
                                     const char *id_or_name,
                                     int force,
                                     CBoxliteError *out_error);

// Get runtime metrics as JSON
enum BoxliteErrorCode boxlite_runtime_metrics(CBoxliteRuntime *runtime,
                                              char **out_json,
                                              CBoxliteError *out_error);

// Gracefully shutdown all boxes in this runtime.
enum BoxliteErrorCode boxlite_runtime_shutdown(CBoxliteRuntime *runtime,
                                               int timeout,
                                               CBoxliteError *out_error);

// Get box info from handle as JSON
enum BoxliteErrorCode boxlite_box_info(CBoxHandle *handle,
                                       char **out_json,
                                       CBoxliteError *out_error);

// Get box metrics from handle as JSON
enum BoxliteErrorCode boxlite_box_metrics(CBoxHandle *handle,
                                          char **out_json,
                                          CBoxliteError *out_error);

// Start or restart a stopped box
enum BoxliteErrorCode boxlite_start_box(CBoxHandle *handle, CBoxliteError *out_error);

// Get box ID string from handle
char *boxlite_box_id(CBoxHandle *handle);

// Create and start a box using simple API
enum BoxliteErrorCode boxlite_simple_new(const char *image,
                                         int cpus,
                                         int memory_mib,
                                         CBoxliteSimple **out_box,
                                         CBoxliteError *out_error);

// Run a command and get buffered result
enum BoxliteErrorCode boxlite_simple_run(CBoxliteSimple *simple_box,
                                         const char *command,
                                         const char *const *args,
                                         int argc,
                                         CBoxliteExecResult **out_result,
                                         CBoxliteError *out_error);

// Free execution result
void boxlite_result_free(CBoxliteExecResult *result);

// Free simple box (auto-cleanup)
void boxlite_simple_free(CBoxliteSimple *simple_box);

// Free a box handle
void boxlite_box_free(CBoxHandle *handle);

// Free a runtime instance
void boxlite_runtime_free(CBoxliteRuntime *runtime);

// Free a string allocated by BoxLite
void boxlite_free_string(char *str);

// Free error struct
void boxlite_error_free(CBoxliteError *error);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* BOXLITE_H */
