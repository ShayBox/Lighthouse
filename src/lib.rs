use std::time::Duration;

use btleplug::{
    api::{Central, Manager as _, Peripheral, ScanFilter, WriteType},
    platform::{Adapter, PeripheralId},
};
use thiserror::Error;
use tokio::time;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct DiscoveredPeripheral {
    pub id: PeripheralId,
    pub name: String,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("BtleError")]
    Btle(#[from] btleplug::Error),
    #[error("StdError")]
    Std(#[from] std::num::ParseIntError),
    #[error("UuidError")]
    Uuid(#[from] uuid::Error),
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
    adapter
        .start_scan(ScanFilter::default())
        .await
        .map_err(Error::Btle)?;

    time::sleep(timeout).await;

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
/// Will return `Err` if `X` fails.
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
