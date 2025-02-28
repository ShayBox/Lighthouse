use std::time::Duration;

use btleplug::{
    api::{Central, Manager as _, Peripheral, ScanFilter},
    platform::Manager,
};
use clap::Parser;
use clap_verbosity_flag::Verbosity;
use lighthouse::Error;
use tokio::time;
use tracing::{info};
use tracing_log::AsTrace;
use uuid::Uuid;

const V1_UUID: &str = "0000cb01-0000-1000-8000-00805f9b34fb";
const V2_UUID: &str = "00001525-1212-efde-1523-785feabcd124";

#[derive(Debug, Parser)]
struct Args {
    /// V1: [OFF|ON] | V2: [OFF|ON|STANDBY]
    #[arg(short, long)]
    state: String,

    /// V1: Basestation BSID (Required) | V2: Bluetooth Device Identifier (Optional)
    #[arg(short, long)]
    bsid: Option<String>,

    #[clap(flatten)]
    verbose: Verbosity,

    /// Request timeout in seconds
    #[arg(short, long, default_value_t = 10)]
    timeout: u64
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Error> {
    let args = Args::parse();

    tracing_subscriber::fmt()
        .with_max_level(args.verbose.log_level_filter().as_trace())
        .init();

    let manager = Manager::new().await.map_err(Error::Btle)?;
    let adapters = manager.adapters().await.map_err(Error::Btle)?;
    if adapters.is_empty() {
        return Err(Error::Message("No Bluetooth adapters found"));
    }

    for adapter in &adapters {
        let info = adapter.adapter_info().await.map_err(Error::Btle)?;
        info!("Starting scan on {info}...");

        adapter
            .start_scan(ScanFilter::default())
            .await
            .map_err(Error::Btle)
            .expect("Can't scan BLE adapter for connected devices...");

        time::sleep(Duration::from_secs(args.timeout)).await;

        let peripherals = adapter.peripherals().await.map_err(Error::Btle)?;
        if peripherals.is_empty() {
            return Err(Error::Message(
                "->>> BLE peripheral devices were not found. Exiting...",
            ));
        }

        for peripheral in &peripherals {
            let Some(properties) = peripheral.properties().await.map_err(Error::Btle)? else {
                continue;
            };

            let Some(name) = properties.local_name else {
                continue;
            };

            let state = args.state.to_uppercase();

            info!("Found '{}' [{}]", name, peripheral.id());

            if name.starts_with("LHB-") // v2
            {
                if let Some(bsid) = &args.bsid
                {
                    if !peripheral.id().to_string().eq_ignore_ascii_case(bsid) {
                        continue;
                    }
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

                let uuid = Uuid::parse_str(V2_UUID).map_err(Error::Uuid)?;

                lighthouse::write(adapter, peripheral.id(), &cmd, uuid).await?;
            }
            else if let Some(bsid) = &args.bsid { // v1
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
                    "OFF" | "STANDBY" => vec![
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

                let uuid = Uuid::parse_str(V1_UUID).map_err(Error::Uuid)?;

                lighthouse::write(adapter, peripheral.id(), &cmd, uuid).await?;
            }
            else { continue; } // not supported
            info!("{} [{}]: {}", name, peripheral.id(), state);
        }
    }
    Ok(())
}
