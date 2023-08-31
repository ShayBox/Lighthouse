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

pub async fn write(
    adapter: &Adapter,
    id: PeripheralId,
    data: &[u8],
    uuid: Uuid,
) -> Result<(), Error> {
    let peripheral = adapter.peripheral(&id).await.map_err(Error::Btle)?;

    if peripheral.connect().await.map_err(Error::Btle).is_ok()
        && peripheral
            .discover_services()
            .await
            .map_err(Error::Btle)
            .is_ok()
    {
        if let Some(characteristic) = peripheral
            .characteristics()
            .into_iter()
            .find(|c| c.uuid == uuid)
        {
            let _ = peripheral
                .write(&characteristic, data, WriteType::WithoutResponse)
                .await
                .map_err(Error::Btle);
        }
    }

    time::sleep(Duration::from_secs(1)).await;
    peripheral.disconnect().await.map_err(Error::Btle)?;

    Ok(())
}
