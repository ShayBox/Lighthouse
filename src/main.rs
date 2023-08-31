use std::time::Duration;

use btleplug::{
    api::{Central, Manager as _, Peripheral, ScanFilter},
    platform::Manager,
};
use clap::Parser;
use clap_verbosity_flag::tracing::Verbosity;
use lighthouse::Error;
use tokio::time;
use tracing::info;
use uuid::Uuid;

#[derive(Debug, Parser)]
struct Args {
    /// V1: [OFF|ON] [BSID] | V2: [OFF|ON|STANDBY]
    #[arg(short, long)]
    state: String,

    /// V1: Basestation BSID
    #[arg(short, long)]
    bsid: Option<String>,

    #[clap(flatten)]
    verbose: Verbosity,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let args = Args::parse();

    tracing_subscriber::fmt()
        .with_max_level(args.verbose.tracing_level_filter())
        .init();

    let manager = Manager::new().await.map_err(Error::Btle)?;
    let adapters = manager.adapters().await.map_err(Error::Btle)?;
    if adapters.is_empty() {
        return Err(Error::Message("No Bluetooth adapters found"));
    }

    for adapter in adapters.iter() {
        let info = adapter.adapter_info().await.map_err(Error::Btle)?;
        info!("Starting scan on {info}...");

        adapter
            .start_scan(ScanFilter::default())
            .await
            .map_err(Error::Btle)
            .expect("Can't scan BLE adapter for connected devices...");
        time::sleep(Duration::from_secs(10)).await;

        let peripherals = adapter.peripherals().await.map_err(Error::Btle)?;
        if peripherals.is_empty() {
            return Err(Error::Message(
                "->>> BLE peripheral devices were not found. Exiting...",
            ));
        }

        for peripheral in peripherals.iter() {
            let Some(properties) = peripheral.properties().await.map_err(Error::Btle)? else {
                continue;
            };

            let Some(name) = properties.local_name else {
                continue;
            };

            let state = args.state.to_uppercase();
            if let Some(bsid) = &args.bsid {
                if !name.starts_with("HTC BS")
                    || name[(name.len() - 4)..] != bsid[(bsid.len() - 4)..]
                {
                    continue;
                }

                let aa = u8::from_str_radix(&bsid[0..2], 16).map_err(Error::Std)?;
                let bb = u8::from_str_radix(&bsid[2..4], 16).map_err(Error::Std)?;
                let cc = u8::from_str_radix(&bsid[4..6], 16).map_err(Error::Std)?;
                let dd = u8::from_str_radix(&bsid[6..8], 16).map_err(Error::Std)?;

                let cmd = match state.as_str() {
                    "OFF" => vec![
                        0x12, 0x02, 0x00, 0x01, dd, cc, bb, aa, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                        0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    ],
                    "ON" => vec![
                        0x12, 0x00, 0x00, 0x00, dd, cc, bb, aa, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                        0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    ],
                    _ => {
                        return Err(Error::Message(
                            "V1: Unknown State {state}, Available: [OFF|ON]",
                        ))
                    }
                };

                const UUID: &str = "0000cb01-0000-1000-8000-00805f9b34fb";
                let uuid = Uuid::parse_str(UUID).map_err(Error::Uuid)?;

                lighthouse::write(adapter, peripheral.id(), &cmd, uuid).await?;
            } else {
                if !name.starts_with("LHB-") {
                    continue;
                }

                let cmd = match state.as_str() {
                    "OFF" => vec![0x00],
                    "ON" => vec![0x01],
                    "STANDBY" => vec![0x02],
                    _ => {
                        return Err(Error::Message(
                            "V2: Unknown State {state}, Available: [OFF|ON|STANDBY]",
                        ))
                    }
                };

                const UUID: &str = "00001525-1212-efde-1523-785feabcd124";
                let uuid = Uuid::parse_str(UUID).map_err(Error::Uuid)?;

                lighthouse::write(adapter, peripheral.id(), &cmd, uuid).await?;
            };
        }
    }
    Ok(())
}
