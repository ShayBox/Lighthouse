use std::ffi::{CString, c_char};

use lighthouse::BaseStationVersion;
use lighthouse::State;
use lighthouse::ffi::{
    LIGHTHOUSE_ABI_VERSION, Handle, LighthouseError, LighthouseState, lighthouse_abi_version,
    lighthouse_detect_version, lighthouse_init, lighthouse_scan, lighthouse_set_channel,
};

#[test]
fn test_state_conversion() {
    assert_eq!(State::from(LighthouseState::Off), State::Off);
    assert_eq!(State::from(LighthouseState::On), State::On);
    assert_eq!(State::from(LighthouseState::Standby), State::Standby);
}

#[test]
fn test_ffi_init_returns_success() {
    let _ = lighthouse_init();
}

#[test]
fn test_scan_null_pointer_returns_error() {
    let mut handles: Vec<Handle> = vec![0; 16];
    let mut count: u32 = 16;
    unsafe {
        let err = lighthouse_scan(
            99999,
            1,
            [std::ptr::null::<c_char>()].as_ptr(),
            1,
            handles.as_mut_ptr(),
            &raw mut count,
        );
        assert_eq!(err, LighthouseError::NullPointer);
        assert_eq!(count, 0);
    }
}

#[test]
fn test_scan_invalid_adapter_handle() {
    let mut handles: Vec<Handle> = vec![0; 16];
    let mut count: u32 = 16;
    let bsid_str = CString::new("test").unwrap();
    let bsids = [bsid_str.as_ptr()];
    unsafe {
        let err = lighthouse_scan(
            99999,
            1,
            bsids.as_ptr(),
            1,
            handles.as_mut_ptr(),
            &raw mut count,
        );
        assert_eq!(err, LighthouseError::InvalidHandle);
        assert_eq!(count, 0);
    }
}

#[test]
fn test_set_channel_invalid_range() {
    unsafe {
        assert_eq!(
            lighthouse_set_channel(1, 2, 0, std::ptr::null(), 1, 1),
            LighthouseError::InvalidChannel
        );
        assert_eq!(
            lighthouse_set_channel(1, 2, 17, std::ptr::null(), 1, 1),
            LighthouseError::InvalidChannel
        );
        assert_eq!(
            lighthouse_set_channel(1, 2, u32::MAX, std::ptr::null(), 1, 1),
            LighthouseError::InvalidChannel
        );
    }
}

#[test]
fn test_set_channel_invalid_handle() {
    unsafe {
        let err = lighthouse_set_channel(99999, 99999, 5, std::ptr::null(), 1, 1);
        assert_eq!(err, LighthouseError::InvalidHandle);
    }
}

#[test]
fn test_abi_version() {
    assert_eq!(lighthouse_abi_version(), LIGHTHOUSE_ABI_VERSION);
}

#[test]
fn test_detect_version() {
    let v2_name = CString::new("LHB-001").unwrap();
    let v1_name = CString::new("HTC BS 001").unwrap();
    let unknown_name = CString::new("Unknown Device").unwrap();
    unsafe {
        assert_eq!(
            lighthouse_detect_version(v2_name.as_ptr()),
            BaseStationVersion::V2,
        );
        assert_eq!(
            lighthouse_detect_version(v1_name.as_ptr()),
            BaseStationVersion::V1,
        );
        assert_eq!(
            lighthouse_detect_version(unknown_name.as_ptr()),
            BaseStationVersion::Unknown,
        );
        assert_eq!(
            lighthouse_detect_version(std::ptr::null()),
            BaseStationVersion::Unknown,
        );
    }
}
