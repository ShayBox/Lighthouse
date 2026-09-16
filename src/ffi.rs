use std::{
    cell::RefCell,
    collections::HashMap,
    ffi::{CStr, CString, c_char},
    sync::{OnceLock, RwLock},
    time::Duration,
};

use btleplug::platform::Adapter;
use tokio::runtime::Runtime;

use crate::{
    Action, BaseStationVersion, DiscoveredPeripheral, Error, State, adapter_info, adapters,
    all_requested_targets_found, process_peripheral, scan_peripherals_until,
};

/// ABI version of the FFI interface.
/// Increment this whenever the C API is changed incompatibly.
pub const LIGHTHOUSE_ABI_VERSION: u32 = 1;

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

fn runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime")
    })
}

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum LighthouseError {
    Success    = 0,
    /// Failed to initialize the runtime
    InitFailed = -1,
    /// No Bluetooth adapters found
    NoAdapters = -2,
    /// Invalid handle
    InvalidHandle = -3,
    /// Scan failed or timed out
    ScanFailed = -4,
    /// Write to device failed
    WriteFailed = -5,
    /// Invalid state enum value
    InvalidState = -6,
    /// Memory allocation failure (`CString`, etc.)
    AllocFailed = -7,
    /// Invalid null pointer argument
    NullPointer = -8,
    /// Invalid channel value (must be 1-16)
    InvalidChannel = -10,
    /// General/other error
    Unknown    = -99,
}

impl From<Error> for LighthouseError {
    fn from(_: Error) -> Self {
        Self::Unknown
    }
}

thread_local! {
    /// Thread-local last error message.
    static LAST_ERROR: RefCell<String> = const { RefCell::new(String::new()) };
}

fn set_last_error(msg: &str) {
    LAST_ERROR.with(|e| {
        let mut err = e.borrow_mut();
        err.clear();
        err.push_str(msg);
    });
}

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum LighthouseState {
    Off     = 0,
    On      = 1,
    Standby = 2,
}

impl From<LighthouseState> for State {
    fn from(state: LighthouseState) -> Self {
        match state {
            LighthouseState::Off => Self::Off,
            LighthouseState::On => Self::On,
            LighthouseState::Standby => Self::Standby,
        }
    }
}

pub type Handle = u64;

struct Arena {
    adapters:    HashMap<Handle, Adapter>,
    peripherals: HashMap<Handle, DiscoveredPeripheral>,
    next_id:     Handle,
}

impl Arena {
    #[must_use]
    fn new() -> Self {
        Self {
            adapters:    HashMap::new(),
            peripherals: HashMap::new(),
            next_id:     1,
        }
    }

    fn alloc_adapter(&mut self, adapter: Adapter) -> Handle {
        let id = self.next_id;
        self.next_id += 1;
        self.adapters.insert(id, adapter);
        id
    }

    fn alloc_peripheral(&mut self, peripheral: DiscoveredPeripheral) -> Handle {
        let id = self.next_id;
        self.next_id += 1;
        self.peripherals.insert(id, peripheral);
        id
    }

    #[allow(clippy::missing_const_for_fn)]
    fn get_adapter(&self, handle: Handle) -> Option<&Adapter> {
        self.adapters.get(&handle)
    }

    fn get_peripheral(&self, handle: Handle) -> Option<&DiscoveredPeripheral> {
        self.peripherals.get(&handle)
    }

    fn release_adapter(&mut self, handle: Handle) -> bool {
        self.adapters.remove(&handle).is_some()
    }

    fn release_peripheral(&mut self, handle: Handle) -> bool {
        self.peripherals.remove(&handle).is_some()
    }
}

static ARENA: OnceLock<RwLock<Arena>> = OnceLock::new();

fn arena() -> &'static RwLock<Arena> {
    ARENA.get_or_init(|| RwLock::new(Arena::new()))
}

/// Initializes the Lighthouse FFI library.
///
/// Creates the internal tokio runtime if not already created.
///
/// # Returns
/// `LighthouseError::Success` on success, or an error code.
#[unsafe(no_mangle)]
pub extern "C" fn lighthouse_init() -> LighthouseError {
    runtime();
    set_last_error("Initialized successfully");
    LighthouseError::Success
}

/// Returns the ABI version of the FFI interface.
///
/// Use this to verify at runtime that the loaded library
/// is compatible with the expected API version.
///
/// # Returns
/// The current ABI version number.
#[unsafe(no_mangle)]
pub const extern "C" fn lighthouse_abi_version() -> u32 {
    LIGHTHOUSE_ABI_VERSION
}

/// Discovers available Bluetooth adapters.
///
/// # Arguments
/// * `handles` - Pre-allocated array of handles to fill. Must be at least `*count` elements.
/// * `count` - On input, the capacity of the handles array. On output, the number of adapters found.
///
/// # Returns
/// Error code. On success, `handles[0..*count]` contains valid adapter handles.
///
/// # Safety
/// `handles` must be a valid pointer to an array of at least `*count` `Handle` elements.
/// `count` must be a valid pointer to a `u32`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lighthouse_discover_adapters(
    handles: *mut Handle,
    count: *mut u32,
) -> LighthouseError {
    if handles.is_null() || count.is_null() {
        unsafe { *count = 0 }
        return LighthouseError::NullPointer;
    }

    let capacity = unsafe { *count };
    let result = runtime().block_on(async {
        match adapters().await {
            Ok(adapters) => Ok(adapters),
            Err(error) => {
                set_last_error(&format!("Failed to discover adapters: {error}"));
                Err(LighthouseError::NoAdapters)
            }
        }
    });

    let adapters = match result {
        Ok(a) => a,
        Err(error) => unsafe {
            *count = 0;
            return error;
        },
    };

    let Ok(mut arena) = arena().write() else {
        unsafe { *count = 0 }
        set_last_error("Arena lock poisoned");
        return LighthouseError::Unknown;
    };

    let num_to_copy = adapters.len().min(capacity as usize);
    let mut handles_written: u32 = 0;

    for adapter in adapters.iter().take(num_to_copy) {
        let handle = arena.alloc_adapter(adapter.clone());
        unsafe { *handles.add(handles_written as usize) = handle }
        handles_written += 1;
    }

    unsafe { *count = handles_written }
    drop(arena);
    set_last_error(&format!("Discovered {handles_written} adapter(s)"));
    LighthouseError::Success
}

/// Scans for Bluetooth peripherals on the given adapter.
///
/// # Arguments
/// * `adapter_handle` - Handle from `lighthouse_discover_adapters`
/// * `timeout_sec` - Scan timeout in seconds
/// * `bsids` - Optional array of BSID strings to filter (can be NULL for no filter)
/// * `bsid_count` - Number of BSID strings
/// * `handles` - Pre-allocated array for peripheral handles
/// * `count` - On input, capacity of handles array. On output, number of peripherals found.
///
/// # Returns
/// Error code.
///
/// # Safety
/// `handles` must be a valid pointer to an array of at least `*count` `Handle` elements.
/// `count` must be a valid pointer to a `u32`.
/// `bsids` if not null must point to an array of `bsid_count` null-terminated strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lighthouse_scan(
    adapter_handle: Handle,
    timeout_sec: u32,
    bsids: *const *const c_char,
    bsid_count: u32,
    handles: *mut Handle,
    count: *mut u32,
) -> LighthouseError {
    if handles.is_null() || count.is_null() {
        unsafe { *count = 0 }
        return LighthouseError::NullPointer;
    }

    let Some(bsid_list) = parse_string_array(bsids, bsid_count) else {
        unsafe { *count = 0 }
        return LighthouseError::NullPointer;
    };

    // Clone adapter out of arena before block_on to avoid holding lock during async ops
    let adapter = {
        let Ok(arena) = arena().read() else {
            unsafe { *count = 0 }
            set_last_error("Arena lock poisoned");
            return LighthouseError::Unknown;
        };
        if let Some(a) = arena.get_adapter(adapter_handle) {
            a.clone()
        } else {
            unsafe { *count = 0 }
            set_last_error("Invalid adapter handle");
            return LighthouseError::InvalidHandle;
        }
    };

    let capacity = unsafe { *count };
    let timeout = Duration::from_secs(u64::from(timeout_sec));

    let result = runtime().block_on(async {
        let peripherals = match scan_peripherals_until(&adapter, timeout, {
            let bsids = bsid_list.clone();
            move |peripherals: &[DiscoveredPeripheral]| -> bool {
                if bsids.is_empty() {
                    return false;
                }
                all_requested_targets_found(peripherals, &bsids)
            }
        })
        .await
        {
            Ok(p) => p,
            Err(error) => {
                set_last_error(&format!("Scan failed: {error}"));
                return Err(LighthouseError::ScanFailed);
            }
        };

        let Ok(mut arena_write) = arena().write() else {
            set_last_error("Arena lock poisoned");
            return Err(LighthouseError::Unknown);
        };
        let num_to_copy = peripherals.len().min(capacity as usize);
        let mut handles_written: u32 = 0;

        for peripheral in peripherals.iter().take(num_to_copy) {
            let handle = arena_write.alloc_peripheral(peripheral.clone());
            unsafe { *handles.add(handles_written as usize) = handle }
            handles_written += 1;
        }

        unsafe { *count = handles_written }
        drop(arena_write);
        set_last_error(&format!("Found {handles_written} peripheral(s)"));
        Ok(())
    });

    match result {
        Ok(()) => LighthouseError::Success,
        Err(error) => unsafe {
            *count = 0;
            error
        },
    }
}

/// Sets the power state of a base station peripheral.
///
/// # Arguments
/// * `adapter_handle` - Handle to the adapter
/// * `peripheral_handle` - Handle to the peripheral
/// * `state` - Desired power state
/// * `bsid` - Optional BSID string (required for V1 devices, ignored for V2). Can be NULL.
/// * `retries` - Number of write attempts (minimum 1)
/// * `retry_delay_sec` - Delay between retry attempts in seconds
///
/// # Returns
/// Error code.
///
/// # Safety
/// `bsid` if not null must be a valid null-terminated UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lighthouse_set_state(
    adapter_handle: Handle,
    peripheral_handle: Handle,
    state: LighthouseState,
    bsid: *const c_char,
    retries: u32,
    retry_delay_sec: u32,
) -> LighthouseError {
    let Ok(bsid_string) = parse_optional_cstring(bsid) else {
        return LighthouseError::Unknown;
    };

    run_action(
        adapter_handle,
        peripheral_handle,
        Action::State(State::from(state)),
        bsid_string.as_deref(),
        retries,
        Duration::from_secs(u64::from(retry_delay_sec)),
    )
}

/// Sets the channel frequency of a V2 base station peripheral.
///
/// # Arguments
/// * `adapter_handle` - Handle to the adapter
/// * `peripheral_handle` - Handle to the peripheral
/// * `channel` - Channel number (1-16)
/// * `bsid` - Optional Bluetooth device identifier filter for V2 devices. Can be NULL.
/// * `retries` - Number of write attempts (minimum 1)
/// * `retry_delay_sec` - Delay between retry attempts in seconds
///
/// # Returns
/// Error code.
///
/// # Safety
/// `bsid` if not null must be a valid null-terminated UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lighthouse_set_channel(
    adapter_handle: Handle,
    peripheral_handle: Handle,
    channel: u32,
    bsid: *const c_char,
    retries: u32,
    retry_delay_sec: u32,
) -> LighthouseError {
    let Ok(channel_u8) = u8::try_from(channel) else {
        set_last_error(&format!("Invalid channel {channel}, must be 1-16"));
        return LighthouseError::InvalidChannel;
    };

    if !matches!(channel_u8, 1..=16) {
        set_last_error(&format!("Invalid channel {channel_u8}, must be 1-16"));
        return LighthouseError::InvalidChannel;
    }

    let Ok(bsid_string) = parse_optional_cstring(bsid) else {
        return LighthouseError::Unknown;
    };

    run_action(
        adapter_handle,
        peripheral_handle,
        Action::Channel(channel_u8),
        bsid_string.as_deref(),
        retries,
        Duration::from_secs(u64::from(retry_delay_sec)),
    )
}

/// Parses an optional null-terminated C string.
///
/// # Safety
/// `ptr` must be NULL or a valid null-terminated UTF-8 string.
fn parse_optional_cstring(ptr: *const c_char) -> Result<Option<String>, LighthouseError> {
    if ptr.is_null() {
        return Ok(None);
    }

    let s = unsafe { CStr::from_ptr(ptr) };
    s.to_str().map(str::to_owned).map(Some).map_err(|_| {
        set_last_error("Invalid string argument (not valid UTF-8)");
        LighthouseError::Unknown
    })
}

/// Resolves handles from the arena and runs a write action on the peripheral.
fn run_action(
    adapter_handle: Handle,
    peripheral_handle: Handle,
    action: Action,
    bsid: Option<&str>,
    retries: u32,
    retry_delay: Duration,
) -> LighthouseError {
    // Clone the data we need out of the arena to avoid holding the lock during async ops
    let (adapter, peripheral) = {
        let Ok(arena) = arena().read() else {
            set_last_error("Arena lock poisoned");
            return LighthouseError::Unknown;
        };
        let Some(adapter) = arena.get_adapter(adapter_handle) else {
            set_last_error("Invalid adapter handle");
            return LighthouseError::InvalidHandle;
        };
        let Some(peripheral) = arena.get_peripheral(peripheral_handle) else {
            set_last_error("Invalid peripheral handle");
            return LighthouseError::InvalidHandle;
        };
        (adapter.clone(), peripheral.clone())
    };

    runtime().block_on(async {
        let bsids: Vec<String> = bsid.map_or_else(Vec::new, |b| vec![b.to_string()]);

        match process_peripheral(
            &adapter,
            &peripheral,
            &action,
            &bsids,
            retries.max(1),
            retry_delay,
        )
        .await
        {
            Ok(Some(desc)) => {
                set_last_error(&format!("Success: {desc}"));
                LighthouseError::Success
            }
            Ok(None) => {
                set_last_error(
                    "Peripheral did not match any target (unknown version or BSID mismatch)",
                );
                LighthouseError::Success
            }
            Err(error) => {
                let code = if matches!(error, Error::InvalidChannel(_)) {
                    LighthouseError::InvalidChannel
                } else {
                    LighthouseError::WriteFailed
                };
                set_last_error(&format!("Write failed: {error}"));
                code
            }
        }
    })
}

/// Gets human-readable adapter info.
///
/// # Arguments
/// * `adapter_handle` - Handle to the adapter
/// * `info_out` - Output pointer. Caller must free with `lighthouse_free_string`.
///
/// # Returns
/// Error code.
///
/// # Safety
/// `info_out` must be a valid pointer to a `*const i8`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lighthouse_adapter_info(
    adapter_handle: Handle,
    info_out: *mut *const c_char,
) -> LighthouseError {
    if info_out.is_null() {
        return LighthouseError::NullPointer;
    }

    // Clone adapter out of arena
    let adapter = {
        let Ok(arena) = arena().read() else {
            set_last_error("Arena lock poisoned");
            return LighthouseError::Unknown;
        };
        if let Some(a) = arena.get_adapter(adapter_handle) {
            a.clone()
        } else {
            set_last_error("Invalid adapter handle");
            return LighthouseError::InvalidHandle;
        }
    };

    let result = runtime().block_on(async {
        match adapter_info(&adapter).await {
            Ok(info) => Ok(info),
            Err(error) => {
                set_last_error(&format!("Failed to get adapter info: {error}"));
                Err(LighthouseError::Unknown)
            }
        }
    });

    match result {
        Ok(info_str) => CString::new(info_str).map_or_else(
            |_| {
                set_last_error("Failed to create CString for adapter info");
                LighthouseError::AllocFailed
            },
            |c_string| {
                unsafe { *info_out = c_string.into_raw() }
                LighthouseError::Success
            },
        ),
        Err(error) => error,
    }
}

/// Gets the name of a peripheral.
///
/// # Arguments
/// * `peripheral_handle` - Handle to the peripheral
/// * `name_out` - Output pointer. Caller must free with `lighthouse_free_string`.
///
/// # Returns
/// Error code.
///
/// # Safety
/// `name_out` must be a valid pointer to a `*const i8`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lighthouse_peripheral_name(
    peripheral_handle: Handle,
    name_out: *mut *const c_char,
) -> LighthouseError {
    if name_out.is_null() {
        return LighthouseError::NullPointer;
    }

    let name = {
        let Ok(arena) = arena().read() else {
            set_last_error("Arena lock poisoned");
            return LighthouseError::Unknown;
        };
        if let Some(p) = arena.get_peripheral(peripheral_handle) {
            p.name.clone()
        } else {
            set_last_error("Invalid peripheral handle");
            return LighthouseError::InvalidHandle;
        }
    };

    CString::new(name).map_or_else(
        |_| {
            set_last_error("Failed to create CString for peripheral name");
            LighthouseError::AllocFailed
        },
        |c_string| {
            unsafe { *name_out = c_string.into_raw() }
            LighthouseError::Success
        },
    )
}

/// Gets the ID string of a peripheral.
///
/// # Arguments
/// * `peripheral_handle` - Handle to the peripheral
/// * `id_out` - Output pointer. Caller must free with `lighthouse_free_string`.
///
/// # Returns
/// Error code.
///
/// # Safety
/// `id_out` must be a valid pointer to a `*const i8`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lighthouse_peripheral_id(
    peripheral_handle: Handle,
    id_out: *mut *const c_char,
) -> LighthouseError {
    if id_out.is_null() {
        return LighthouseError::NullPointer;
    }

    let id_str = {
        let Ok(arena) = arena().read() else {
            set_last_error("Arena lock poisoned");
            return LighthouseError::Unknown;
        };
        if let Some(p) = arena.get_peripheral(peripheral_handle) {
            p.id.to_string()
        } else {
            set_last_error("Invalid peripheral handle");
            return LighthouseError::InvalidHandle;
        }
    };

    CString::new(id_str).map_or_else(
        |_| {
            set_last_error("Failed to create CString for peripheral id");
            LighthouseError::AllocFailed
        },
        |c_string| {
            unsafe { *id_out = c_string.into_raw() }
            LighthouseError::Success
        },
    )
}

/// Detects the base station version from a device name.
///
/// # Arguments
/// * `name` - Device name string
///
/// # Returns
/// `BaseStationVersion` value.
///
/// # Safety
/// `name` must be a valid null-terminated UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lighthouse_detect_version(name: *const c_char) -> BaseStationVersion {
    if name.is_null() {
        return BaseStationVersion::Unknown;
    }
    let c_str = unsafe { CStr::from_ptr(name.cast::<c_char>()) };
    let Ok(name_str) = c_str.to_str() else {
        return BaseStationVersion::Unknown;
    };

    BaseStationVersion::detect(name_str)
}

/// Releases a handle, freeing the associated resources.
///
/// # Arguments
/// * `handle` - The handle to release
///
/// # Returns
/// Error code. Returns `LighthouseError::Success` if the handle was valid and released.
#[unsafe(no_mangle)]
pub extern "C" fn lighthouse_release_handle(handle: Handle) -> LighthouseError {
    let Ok(mut arena) = arena().write() else {
        set_last_error("Arena lock poisoned");
        return LighthouseError::Unknown;
    };

    if arena.release_adapter(handle) {
        set_last_error("Released adapter handle");
        return LighthouseError::Success;
    }
    if arena.release_peripheral(handle) {
        set_last_error("Released peripheral handle");
        return LighthouseError::Success;
    }

    drop(arena);
    set_last_error("Handle not found (already released or invalid)");
    LighthouseError::InvalidHandle
}

/// Frees a string that was allocated by the library.
///
/// # Arguments
/// * `ptr` - Pointer returned from an FFI function (e.g., `lighthouse_adapter_info`).
///
/// # Safety
/// The pointer must have been returned by an FFI function that documents
/// that the caller is responsible for freeing it. Must not be called twice
/// on the same pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lighthouse_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        let _ = unsafe { CString::from_raw(ptr) };
    }
}

/// Gets the last error message.
///
/// # Returns
/// A null-terminated string. The pointer is valid until the next FFI call
/// on the same thread. Do not free.
#[unsafe(no_mangle)]
pub extern "C" fn lighthouse_last_error() -> *const c_char {
    LAST_ERROR.with(|e| e.borrow().as_ptr().cast::<c_char>())
}

/// Checks if all requested BSID targets have been found in a list of peripherals.
///
/// # Arguments
/// * `peripheral_handles` - Array of peripheral handles
/// * `peripheral_count` - Number of peripherals
/// * `bsids` - Array of BSID strings
/// * `bsid_count` - Number of BSIDs
///
/// # Returns
/// 1 if all targets found, 0 otherwise, -1 on error.
///
/// # Safety
/// `peripheral_handles` must be a valid array of `peripheral_count` elements.
/// `bsids` if not null must point to an array of `bsid_count` null-terminated strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lighthouse_all_targets_found(
    peripheral_handles: *const Handle,
    peripheral_count: u32,
    bsids: *const *const c_char,
    bsid_count: u32,
) -> i32 {
    let Some(bsid_list) = parse_string_array(bsids, bsid_count) else {
        return -1;
    };

    if peripheral_handles.is_null() || peripheral_count == 0 {
        return i32::from(bsid_list.is_empty());
    }

    let Ok(arena) = arena().read() else {
        return -1;
    };

    let mut discovered = Vec::new();

    for i in 0..peripheral_count as usize {
        let handle = unsafe { *peripheral_handles.add(i) };
        if let Some(p) = arena.get_peripheral(handle) {
            discovered.push(p.clone());
        }
    }

    i32::from(all_requested_targets_found(&discovered, &bsid_list))
}

/// Parse a null-terminated array of `*const c_char` into `Vec<String>`.
///
/// # Safety
/// `ptr` must point to an array of `count` null-terminated strings, or be NULL.
fn parse_string_array(ptr: *const *const c_char, count: u32) -> Option<Vec<String>> {
    if ptr.is_null() || count == 0 {
        return Some(Vec::new());
    }
    unsafe {
        let slice = std::slice::from_raw_parts(ptr, count as usize);
        let mut result = Vec::with_capacity(slice.len());
        for s in slice {
            if s.is_null() {
                return None;
            }
            let c_str = CStr::from_ptr(*s);
            let string = c_str.to_string_lossy().into_owned();
            result.push(string);
        }
        Some(result)
    }
}
