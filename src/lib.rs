use std::time::Duration;

use btleplug::{
    api::{Central, Peripheral, WriteType},
    platform::{Adapter, PeripheralId},
};
use thiserror::Error;
use tokio::time;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum Error {
    #[error("BtleError")]
    Btle(#[from] btleplug::Error),
    #[error("StdError")]
    Std(#[from] std::num::ParseIntError),
    #[error("UuidError")]
    Uuid(#[from] uuid::Error),
    #[error("{0}")]
    Message(&'static str),
}

/// # Write to a device
///
/// # Errors
/// Will return `Err` if `X` fails.
pub async fn write(
    adapter: &Adapter,
    id: PeripheralId,
    data: &[u8],
    uuid: Uuid,
) -> Result<(), Error> {
    let peripheral = adapter.peripheral(&id).await.map_err(Error::Btle)?;

    if peripheral.connect().await.map_err(Error::Btle).is_err() {
        return Err(Error::Message("Failed to connect"));
    }

    if peripheral
        .discover_services()
        .await
        .map_err(Error::Btle)
        .is_err()
    {
        peripheral.disconnect().await.map_err(Error::Btle)?;
        return Err(Error::Message("Failed to scan"));
    }

    let characteristic = peripheral
        .characteristics()
        .into_iter()
        .find(|c| c.uuid == uuid);

    if let Some(characteristic) = characteristic {
        if peripheral
            .write(&characteristic, data, WriteType::WithoutResponse)
            .await
            .map_err(Error::Btle)
            .is_err()
        {
            return Err(Error::Message("Failed to write"));
        }
    }

    time::sleep(Duration::from_secs(1)).await;
    peripheral.disconnect().await.map_err(Error::Btle)?;

    Ok(())
}
