use std::{fmt, str::FromStr, sync::LazyLock, time::Duration};

use btleplug::{
    api::{Central, Manager as _, Peripheral, ScanFilter, WriteType},
    platform::{Adapter, PeripheralId},
};
use thiserror::Error;
use tokio::time;
use uuid::Uuid;

/// Parsed UUID for V1 base station characteristic
pub static V1_UUID: LazyLock<Uuid> = LazyLock::new(|| {
    Uuid::parse_str("0000cb01-0000-1000-8000-00805f9b34fb").expect("V1 UUID is a valid literal")
});

/// Parsed UUID for V2 base station characteristic
pub static V2_UUID: LazyLock<Uuid> = LazyLock::new(|| {
    Uuid::parse_str("00001525-1212-efde-1523-785feabcd124").expect("V2 UUID is a valid literal")
});

/// Base station version detection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaseStationVersion {
    V1,
    V2,
}

impl BaseStationVersion {
    /// Detect the base station version from the device name.
    /// - V2 devices start with "LHB-"
    /// - V1 devices start with "HTC BS"
    #[must_use]
    pub fn detect(name: &str) -> Option<Self> {
        if name.starts_with("LHB-") {
            Some(Self::V2)
        } else if name.starts_with("HTC BS") {
            Some(Self::V1)
        } else {
            None
        }
    }

    /// Returns the characteristic UUID for this base station version
    #[must_use]
    pub fn uuid(&self) -> &'static Uuid {
        match self {
            Self::V1 => &V1_UUID,
            Self::V2 => &V2_UUID,
        }
    }

    /// Check if a peripheral matches the requested BSID inputs for this version.
    ///
    /// For V1, returns the matched BSID (needed for command generation).
    /// For V2, returns `Some(())` as an empty string marker if the peripheral matches,
    /// or `None` if it does not match.
    #[must_use]
    pub fn matches_bsid(&self, name: &str, peripheral_id: &str, bsids: &[String]) -> Option<String> {
        match self {
            Self::V1 => matches_v1_bsid(name, bsids).map(String::from),
            Self::V2 => {
                if matches_v2_bsid(peripheral_id, Some(bsids)) {
                    Some(String::new())
                } else {
                    None
                }
            }
        }
    }

    /// Generates the command bytes for this base station version.
    ///
    /// # Arguments
    /// * `state` - The desired power state
    /// * `bsid` - An 8-character hex BSID (required for V1, ignored for V2)
    ///
    /// # Errors
    /// Returns `Error::InvalidState` if STANDBY is used with V1.
    /// Returns `Error::Std` if the V1 BSID contains invalid hex characters.
    pub fn command(&self, state: &State, bsid: Option<&str>) -> Result<Vec<u8>, Error> {
        match self {
            Self::V1 => {
                let Some(bsid) = bsid else {
                    return Err(Error::Message("V1 requires a BSID".into()));
                };
                v1_command(state, bsid)
            }
            Self::V2 => v2_command(state),
        }
    }
}

/// Power state for base stations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Off,
    On,
    Standby,
}

impl State {
    /// Parse a state from a string (case-insensitive).
    /// Returns `None` if the string doesn't match a known state.
    #[must_use]
    pub fn try_from_str(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "OFF" => Some(Self::Off),
            "ON" => Some(Self::On),
            "STANDBY" => Some(Self::Standby),
            _ => None,
        }
    }
}

impl FromStr for State {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from_str(s).ok_or_else(|| {
            Error::InvalidState(format!("Unknown state '{s}', available: [OFF|ON|STANDBY]"))
        })
    }
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Off => f.write_str("OFF"),
            Self::On => f.write_str("ON"),
            Self::Standby => f.write_str("STANDBY"),
        }
    }
}

/// Normalizes a BSID input by lowercasing and replacing underscores with colons.
/// This allows user input in either format (e.g., `E2_5A_B0` or `e2:5a:b0`).
#[must_use]
pub fn normalize_bsid(input: &str) -> String {
    input.to_lowercase().replace('_', ":")
}

/// Normalizes a peripheral ID for comparison.
/// On Linux, peripheral IDs look like `hci0/dev_E2_5A_B0_E4_97_AD`.
/// This normalizes to lowercase with colons for consistent matching.
#[must_use]
pub fn normalize_peripheral_id(peripheral_id: &str) -> String {
    peripheral_id.to_lowercase().replace('_', ":")
}

/// Generates a V1 base station command.
///
/// # Arguments
/// * `state` - The desired power state
/// * `bsid` - An 8-character hex BSID (e.g., "aabbccdd")
///
/// # Errors
/// Returns `Error::Std` if the BSID contains invalid hex characters.
/// Returns `Error::InvalidState` if STANDBY is used with V1 (V1 only supports ON/OFF).
pub fn v1_command(state: &State, bsid: &str) -> Result<Vec<u8>, Error> {
    if matches!(state, State::Standby) {
        return Err(Error::InvalidState(
            "V1 base stations do not support STANDBY, use OFF or ON".into(),
        ));
    }

    let aa = u8::from_str_radix(&bsid[0..2], 16).map_err(Error::Std)?;
    let bb = u8::from_str_radix(&bsid[2..4], 16).map_err(Error::Std)?;
    let cc = u8::from_str_radix(&bsid[4..6], 16).map_err(Error::Std)?;
    let dd = u8::from_str_radix(&bsid[6..8], 16).map_err(Error::Std)?;

    match state {
        State::Off => Ok(vec![
            0x12, 0x02, 0x00, 0x01, dd, cc, bb, aa, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
        ]),
        State::On => Ok(vec![
            0x12, 0x00, 0x00, 0x00, dd, cc, bb, aa, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
        ]),
        State::Standby => unreachable!(),
    }
}

/// Generates a V2 base station command.
///
/// # Errors
/// Returns `Error::InvalidState` if an unsupported state is provided.
pub fn v2_command(state: &State) -> Result<Vec<u8>, Error> {
    match state {
        State::Off => Ok(vec![0x00]),
        State::On => Ok(vec![0x01]),
        State::Standby => Ok(vec![0x02]),
    }
}

/// Checks if a peripheral matches any of the provided V1 BSIDs.
/// Returns the matching BSID if found.
///
/// V1 matching compares the last 4 characters of the device name
/// against the last 4 characters of each BSID.
#[must_use]
pub fn matches_v1_bsid<'a>(name: &str, bsids: &'a [String]) -> Option<&'a str> {
    if bsids.is_empty() {
        return None;
    }

    if !name.starts_with("HTC BS") || name.len() < 4 {
        return None;
    }

    bsids.iter().find_map(|bsid| {
        let bsid = bsid.trim();
        if bsid.len() != 8 {
            return None;
        }
        if name[(name.len() - 4)..] != bsid[(bsid.len() - 4)..] {
            return None;
        }
        Some(bsid)
    })
}

/// Checks if a peripheral ID matches any of the normalized V2 BSID inputs.
///
/// If `normalized_inputs` is `None`, all V2 devices match.
#[must_use]
pub fn matches_v2_bsid(peripheral_id: &str, normalized_inputs: Option<&[String]>) -> bool {
    let Some(normalized_inputs) = normalized_inputs else {
        return true;
    };

    let normalized_peripheral_id = normalize_peripheral_id(peripheral_id);
    normalized_inputs
        .iter()
        .any(|input| normalized_peripheral_id.contains(input.as_str()))
}

/// Processes a single peripheral: detects version, matches BSID, builds command, and writes.
///
/// Returns `Ok(Some(desc))` with a description string on success, `Ok(None)` if the peripheral
/// was skipped (unknown version or no BSID match), or `Err` if the write failed.
///
/// # Arguments
/// * `adapter` - The Bluetooth adapter
/// * `peripheral` - The discovered peripheral to process
/// * `state` - The desired power state
/// * `bsids` - The raw (unnormalized) BSID inputs from the user
/// * `retries` - Number of write attempts (minimum 1)
/// * `retry_delay` - Delay between failed attempts
///
/// # Errors
/// Returns `Error` if command generation or writing fails.
pub async fn process_peripheral(
    adapter: &Adapter,
    peripheral: &DiscoveredPeripheral,
    state: &State,
    bsids: &[String],
    retries: u32,
    retry_delay: Duration,
) -> Result<Option<String>, Error> {
    let peripheral_id_str = peripheral.id.to_string();

    let Some(version) = BaseStationVersion::detect(&peripheral.name) else {
        return Ok(None);
    };

    // When no BSIDs provided: V2 matches all, V1 matches none
    let bsid = if bsids.is_empty() {
        match version {
            BaseStationVersion::V2 => Some(String::new()),
            BaseStationVersion::V1 => None,
        }
    } else {
        version.matches_bsid(&peripheral.name, &peripheral_id_str, bsids)
    };

    let Some(bsid) = bsid else {
        return Ok(None);
    };

    // V2 returns empty string (no BSID needed), V1 returns the actual BSID
    let bsid_for_cmd = if bsid.is_empty() { None } else { Some(bsid.as_str()) };
    let cmd = version.command(state, bsid_for_cmd)?;
    let uuid = *version.uuid();

    write_with_retries(
        adapter,
        peripheral,
        &cmd,
        uuid,
        retries.max(1),
        retry_delay,
    )
    .await?;

    Ok(Some(format!(
        "{} [{}]: {state}",
        peripheral.name,
        peripheral_id_str
    )))
}

/// Checks if all requested targets have been found in the discovered peripherals.
#[must_use]
pub fn all_requested_targets_found(
    peripherals: &[DiscoveredPeripheral],
    bsids: &[String],
) -> bool {
    if bsids.is_empty() {
        return false;
    }

    bsids.iter().all(|bsid| {
        peripherals.iter().any(|peripheral| {
            let peripheral_id_str = peripheral.id.to_string();
            let Some(version) = BaseStationVersion::detect(&peripheral.name) else {
                return false;
            };

            version.matches_bsid(
                &peripheral.name,
                &peripheral_id_str,
                std::slice::from_ref(bsid),
            ).is_some()
        })
    })
}

/// Writes data to a peripheral with retry logic.
///
/// # Arguments
/// * `adapter` - The Bluetooth adapter
/// * `peripheral` - The discovered peripheral
/// * `cmd` - The command bytes to write
/// * `uuid` - The characteristic UUID to write to
/// * `retries` - Number of write attempts (minimum 1)
/// * `retry_delay` - Delay between failed attempts
///
/// # Errors
/// Returns `Error::WriteAllFailed` if all retry attempts fail.
pub async fn write_with_retries(
    adapter: &Adapter,
    peripheral: &DiscoveredPeripheral,
    cmd: &[u8],
    uuid: Uuid,
    retries: u32,
    retry_delay: Duration,
) -> Result<(), Error> {
    for attempt in 1..=retries {
        match write(adapter, &peripheral.id, cmd, uuid).await {
            Ok(()) => return Ok(()),
            Err(error) if attempt == retries => return Err(error),
            #[allow(unused_variables)]
            Err(error) => {
                #[cfg(feature = "tracing")]
                tracing::warn!(
                    attempt,
                    retries,
                    name = %peripheral.name,
                    id = %peripheral.id,
                    %error,
                    "Write attempt failed"
                );
                time::sleep(retry_delay).await;
            }
        }
    }

    Err(Error::Message(String::from("No write attempts were made")))
}

#[derive(Debug, Clone)]
pub struct DiscoveredPeripheral {
    pub id:   PeripheralId,
    pub name: String,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Bluetooth error: {0}")]
    Btle(#[from] btleplug::Error),

    #[error("Parse error: {0}")]
    Std(#[from] std::num::ParseIntError),

    #[error("UUID error: {0}")]
    Uuid(#[from] uuid::Error),

    #[error("Invalid state: {0}")]
    InvalidState(String),

    #[error("{0}")]
    Message(String),
}

/// # Errors
/// Returns `Err` if the bluetooth manager or adapter list fails.
pub async fn adapters() -> Result<Vec<Adapter>, Error> {
    let manager = btleplug::platform::Manager::new()
        .await
        .map_err(Error::Btle)?;
    manager.adapters().await.map_err(Error::Btle)
}

/// # Errors
/// Returns `Err` if the adapter info cannot be read.
pub async fn adapter_info(adapter: &Adapter) -> Result<String, Error> {
    adapter.adapter_info().await.map_err(Error::Btle)
}

/// # Errors
/// Returns `Err` if scanning or peripheral enumeration fails.
pub async fn scan_peripherals(
    adapter: &Adapter,
    timeout: Duration,
) -> Result<Vec<DiscoveredPeripheral>, Error> {
    scan_peripherals_until(adapter, timeout, |_| false).await
}

/// # Errors
/// Returns `Err` if scanning or peripheral enumeration fails.
pub async fn scan_peripherals_until(
    adapter: &Adapter,
    timeout: Duration,
    mut is_complete: impl FnMut(&[DiscoveredPeripheral]) -> bool,
) -> Result<Vec<DiscoveredPeripheral>, Error> {
    adapter
        .start_scan(ScanFilter::default())
        .await
        .map_err(Error::Btle)?;

    let scan_started = time::Instant::now();
    let mut discovered = Vec::new();
    while scan_started.elapsed() < timeout {
        time::sleep(Duration::from_millis(250)).await;
        discovered = discovered_peripherals(adapter).await?;
        if is_complete(&discovered) {
            break;
        }
    }

    #[allow(unused_variables)]
    if let Err(error) = adapter.stop_scan().await {
        #[cfg(feature = "tracing")]
        tracing::debug!(%error, "Failed to stop scan");
    }

    if discovered.is_empty() {
        discovered = discovered_peripherals(adapter).await?;
    }

    Ok(discovered)
}

async fn discovered_peripherals(adapter: &Adapter) -> Result<Vec<DiscoveredPeripheral>, Error> {
    let peripherals = adapter.peripherals().await.map_err(Error::Btle)?;
    let mut discovered = Vec::new();
    for peripheral in peripherals {
        let Ok(Some(properties)) = peripheral.properties().await else {
            continue;
        };
        let Some(name) = properties.local_name else {
            continue;
        };

        discovered.push(DiscoveredPeripheral {
            id: peripheral.id(),
            name,
        });
    }

    Ok(discovered)
}

/// # Write to a device
///
/// # Errors
/// Will return `Err` if connection, service discovery, or write fails.
pub async fn write(
    adapter: &Adapter,
    id: &PeripheralId,
    data: &[u8],
    uuid: Uuid,
) -> Result<(), Error> {
    let peripheral = adapter.peripheral(id).await.map_err(Error::Btle)?;

    if peripheral.connect().await.map_err(Error::Btle).is_err() {
        return Err(Error::Message(String::from("Failed to connect")));
    }

    if peripheral
        .discover_services()
        .await
        .map_err(Error::Btle)
        .is_err()
    {
        peripheral.disconnect().await.map_err(Error::Btle)?;
        return Err(Error::Message(String::from("Failed to scan")));
    }

    let characteristic = peripheral
        .characteristics()
        .into_iter()
        .find(|c| c.uuid == uuid);

    if let Some(characteristic) = characteristic
        && peripheral
            .write(&characteristic, data, WriteType::WithoutResponse)
            .await
            .map_err(Error::Btle)
            .is_err()
    {
        return Err(Error::Message(String::from("Failed to write")));
    }

    time::sleep(Duration::from_secs(1)).await;
    peripheral.disconnect().await.map_err(Error::Btle)?;

    Ok(())
}
