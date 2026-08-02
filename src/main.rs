use std::{str::FromStr, time::Duration};

use clap::Parser;
use clap_verbosity_flag::Verbosity;
use lighthouse::{
    Error,
    State,
    all_requested_targets_found,
    process_peripheral,
};
use tracing::info;
use tracing_log::AsTrace;

#[derive(Debug, Parser)]
struct Args {
    /// V1: [OFF|ON] | V2: [OFF|ON|STANDBY]
    #[arg(short, long)]
    state: String,

    /// V1: Base Station BSID (Required) | V2: Bluetooth Device Identifier (Optional)
    #[arg(short, long, value_delimiter = ',', num_args = 1..)]
    bsid: Vec<String>,

    #[clap(flatten)]
    verbose: Verbosity,

    /// Request timeout in seconds
    #[arg(short, long, default_value_t = 10)]
    timeout: u64,

    /// Number of write attempts per base station
    #[arg(long, default_value_t = 3)]
    retries: u32,

    /// Delay between write attempts in seconds
    #[arg(long, default_value_t = 2)]
    retry_delay: u64,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Error> {
    let args = Args::parse();

    tracing_subscriber::fmt()
        .with_max_level(args.verbose.log_level_filter().as_trace())
        .init();

    let state = State::from_str(&args.state)?;

    let adapters = lighthouse::adapters().await?;
    if adapters.is_empty() {
        return Err(Error::Message(String::from("No Bluetooth adapters found")));
    }

    for adapter in &adapters {
        let info = lighthouse::adapter_info(adapter).await?;
        info!("Starting scan on {info}...");

        let peripherals = lighthouse::scan_peripherals_until(
            adapter,
            Duration::from_secs(args.timeout),
            |peripherals| {
                all_requested_targets_found(peripherals, &args.bsid)
            },
        )
        .await?;

        if peripherals.is_empty() {
            return Err(Error::Message(String::from(
                "->>> BLE peripheral devices were not found. Exiting...",
            )));
        }

        for peripheral in &peripherals {
            let peripheral_id_str = peripheral.id.to_string();
            info!("Found '{}' [{}]", peripheral.name, peripheral_id_str);

            match process_peripheral(
                adapter,
                peripheral,
                &state,
                &args.bsid,
                args.retries.max(1),
                Duration::from_secs(args.retry_delay),
            )
            .await?
            {
                Some(desc) => info!("{desc}"),
                None => continue,
            }
        }
    }

    Ok(())
}