#ifndef LIGHTHOUSE_H
#define LIGHTHOUSE_H

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

/**
 * ABI version of the FFI interface.
 * Increment this whenever the C API is changed incompatibly.
 */
#define LIGHTHOUSE_ABI_VERSION 1

typedef enum {
  Success = 0,
  /**
   * Failed to initialize the runtime
   */
  InitFailed = -1,
  /**
   * No Bluetooth adapters found
   */
  NoAdapters = -2,
  /**
   * Invalid handle
   */
  InvalidHandle = -3,
  /**
   * Scan failed or timed out
   */
  ScanFailed = -4,
  /**
   * Write to device failed
   */
  WriteFailed = -5,
  /**
   * Invalid state enum value
   */
  InvalidState = -6,
  /**
   * Memory allocation failure (`CString`, etc.)
   */
  AllocFailed = -7,
  /**
   * Invalid null pointer argument
   */
  NullPointer = -8,
  /**
   * Invalid channel value (must be 1-16)
   */
  InvalidChannel = -10,
  /**
   * General/other error
   */
  Unknown = -99,
} LighthouseError;

typedef enum {
  Off = 0,
  On = 1,
  Standby = 2,
} LighthouseState;

/**
 * Base station version detection
 */
typedef enum {
  Unknown = 0,
  V1 = 1,
  V2 = 2,
} BaseStationVersion;

typedef uint64_t Handle;

/**
 * Initializes the Lighthouse FFI library.
 *
 * Creates the internal tokio runtime if not already created.
 *
 * # Returns
 * `LighthouseError::Success` on success, or an error code.
 */
LighthouseError lighthouse_init(void);

/**
 * Returns the ABI version of the FFI interface.
 *
 * Use this to verify at runtime that the loaded library
 * is compatible with the expected API version.
 *
 * # Returns
 * The current ABI version number.
 */
uint32_t lighthouse_abi_version(void);

/**
 * Discovers available Bluetooth adapters.
 *
 * # Arguments
 * * `handles` - Pre-allocated array of handles to fill. Must be at least `*count` elements.
 * * `count` - On input, the capacity of the handles array. On output, the number of adapters found.
 *
 * # Returns
 * Error code. On success, `handles[0..*count]` contains valid adapter handles.
 *
 * # Safety
 * `handles` must be a valid pointer to an array of at least `*count` `Handle` elements.
 * `count` must be a valid pointer to a `u32`.
 */
LighthouseError lighthouse_discover_adapters(Handle *handles, uint32_t *count);

/**
 * Scans for Bluetooth peripherals on the given adapter.
 *
 * # Arguments
 * * `adapter_handle` - Handle from `lighthouse_discover_adapters`
 * * `timeout_sec` - Scan timeout in seconds
 * * `bsids` - Optional array of BSID strings to filter (can be NULL for no filter)
 * * `bsid_count` - Number of BSID strings
 * * `handles` - Pre-allocated array for peripheral handles
 * * `count` - On input, capacity of handles array. On output, number of peripherals found.
 *
 * # Returns
 * Error code.
 *
 * # Safety
 * `handles` must be a valid pointer to an array of at least `*count` `Handle` elements.
 * `count` must be a valid pointer to a `u32`.
 * `bsids` if not null must point to an array of `bsid_count` null-terminated strings.
 */
LighthouseError lighthouse_scan(Handle adapter_handle,
                                uint32_t timeout_sec,
                                const char *const *bsids,
                                uint32_t bsid_count,
                                Handle *handles,
                                uint32_t *count);

/**
 * Sets the power state of a base station peripheral.
 *
 * # Arguments
 * * `adapter_handle` - Handle to the adapter
 * * `peripheral_handle` - Handle to the peripheral
 * * `state` - Desired power state
 * * `bsid` - Optional BSID string (required for V1 devices, ignored for V2). Can be NULL.
 * * `retries` - Number of write attempts (minimum 1)
 * * `retry_delay_sec` - Delay between retry attempts in seconds
 *
 * # Returns
 * Error code.
 *
 * # Safety
 * `bsid` if not null must be a valid null-terminated UTF-8 string.
 */
LighthouseError lighthouse_set_state(Handle adapter_handle,
                                     Handle peripheral_handle,
                                     LighthouseState state,
                                     const char *bsid,
                                     uint32_t retries,
                                     uint32_t retry_delay_sec);

/**
 * Sets the channel frequency of a V2 base station peripheral.
 *
 * # Arguments
 * * `adapter_handle` - Handle to the adapter
 * * `peripheral_handle` - Handle to the peripheral
 * * `channel` - Channel number (1-16)
 * * `bsid` - Optional Bluetooth device identifier filter for V2 devices. Can be NULL.
 * * `retries` - Number of write attempts (minimum 1)
 * * `retry_delay_sec` - Delay between retry attempts in seconds
 *
 * # Returns
 * Error code.
 *
 * # Safety
 * `bsid` if not null must be a valid null-terminated UTF-8 string.
 */
LighthouseError lighthouse_set_channel(Handle adapter_handle,
                                       Handle peripheral_handle,
                                       uint32_t channel,
                                       const char *bsid,
                                       uint32_t retries,
                                       uint32_t retry_delay_sec);

/**
 * Gets human-readable adapter info.
 *
 * # Arguments
 * * `adapter_handle` - Handle to the adapter
 * * `info_out` - Output pointer. Caller must free with `lighthouse_free_string`.
 *
 * # Returns
 * Error code.
 *
 * # Safety
 * `info_out` must be a valid pointer to a `*const i8`.
 */
LighthouseError lighthouse_adapter_info(Handle adapter_handle, const char **info_out);

/**
 * Gets the name of a peripheral.
 *
 * # Arguments
 * * `peripheral_handle` - Handle to the peripheral
 * * `name_out` - Output pointer. Caller must free with `lighthouse_free_string`.
 *
 * # Returns
 * Error code.
 *
 * # Safety
 * `name_out` must be a valid pointer to a `*const i8`.
 */
LighthouseError lighthouse_peripheral_name(Handle peripheral_handle, const char **name_out);

/**
 * Gets the ID string of a peripheral.
 *
 * # Arguments
 * * `peripheral_handle` - Handle to the peripheral
 * * `id_out` - Output pointer. Caller must free with `lighthouse_free_string`.
 *
 * # Returns
 * Error code.
 *
 * # Safety
 * `id_out` must be a valid pointer to a `*const i8`.
 */
LighthouseError lighthouse_peripheral_id(Handle peripheral_handle, const char **id_out);

/**
 * Detects the base station version from a device name.
 *
 * # Arguments
 * * `name` - Device name string
 *
 * # Returns
 * `BaseStationVersion` value.
 *
 * # Safety
 * `name` must be a valid null-terminated UTF-8 string.
 */
BaseStationVersion lighthouse_detect_version(const char *name);

/**
 * Releases a handle, freeing the associated resources.
 *
 * # Arguments
 * * `handle` - The handle to release
 *
 * # Returns
 * Error code. Returns `LighthouseError::Success` if the handle was valid and released.
 */
LighthouseError lighthouse_release_handle(Handle handle);

/**
 * Frees a string that was allocated by the library.
 *
 * # Arguments
 * * `ptr` - Pointer returned from an FFI function (e.g., `lighthouse_adapter_info`).
 *
 * # Safety
 * The pointer must have been returned by an FFI function that documents
 * that the caller is responsible for freeing it. Must not be called twice
 * on the same pointer.
 */
void lighthouse_free_string(char *ptr);

/**
 * Gets the last error message.
 *
 * # Returns
 * A null-terminated string. The pointer is valid until the next FFI call
 * on the same thread. Do not free.
 */
const char *lighthouse_last_error(void);

/**
 * Checks if all requested BSID targets have been found in a list of peripherals.
 *
 * # Arguments
 * * `peripheral_handles` - Array of peripheral handles
 * * `peripheral_count` - Number of peripherals
 * * `bsids` - Array of BSID strings
 * * `bsid_count` - Number of BSIDs
 *
 * # Returns
 * 1 if all targets found, 0 otherwise, -1 on error.
 *
 * # Safety
 * `peripheral_handles` must be a valid array of `peripheral_count` elements.
 * `bsids` if not null must point to an array of `bsid_count` null-terminated strings.
 */
int32_t lighthouse_all_targets_found(const Handle *peripheral_handles,
                                     uint32_t peripheral_count,
                                     const char *const *bsids,
                                     uint32_t bsid_count);

#endif  /* LIGHTHOUSE_H */
