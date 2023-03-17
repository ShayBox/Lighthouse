use std::{fmt, time::Duration};

use btleplug::{
    api::{Central, Peripheral, WriteType},
    platform::{Adapter, PeripheralId},
};
use error_stack::{Context, IntoReport, Result, ResultExt};
use tokio::time;
use uuid::Uuid;

#[derive(Debug)]
pub enum Error {
    Btle,
    Std,
    Uuid,
    Message(&'static str),
}

impl Context for Error {}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self {
            Error::Btle => fmt.write_str("BtleError"),
            Error::Std => fmt.write_str("StdError"),
            Error::Uuid => fmt.write_str("UuidError"),
            Error::Message(data) => fmt.write_str(data),
        }
    }
}

pub async fn write(
    adapter: &Adapter,
    id: PeripheralId,
    data: &[u8],
    uuid: Uuid,
) -> Result<(), Error> {
    let peripheral = adapter
        .peripheral(&id)
        .await
        .into_report()
        .change_context(Error::Btle)?;

    if peripheral
        .connect()
        .await
        .into_report()
        .change_context(Error::Btle)
        .is_ok()
        && peripheral
            .discover_services()
            .await
            .into_report()
            .change_context(Error::Btle)
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
                .into_report()
                .change_context(Error::Btle);
        }
    }

    time::sleep(Duration::from_secs(1)).await;
    peripheral
        .disconnect()
        .await
        .into_report()
        .change_context(Error::Btle)?;

    Ok(())
}
