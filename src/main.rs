use std::{str::FromStr, time::Duration};

use clap::{ArgGroup, Parser};
use clap_verbosity_flag::Verbosity;
use lighthouse::{
    Action, BaseStationVersion, Error, State, all_requested_targets_found, process_peripheral,
};
use tracing::{info, warn};
use tracing_log::AsTrace;

#[derive(Debug, Parser)]
#[command(
    about = "Virtual reality basestation power management",
    group = ArgGroup::new("action").args(["state", "channel"])
)]
struct Args {
    /// V1: [OFF|ON] | V2: [OFF|ON|STANDBY] (omit both action flags to show status)
    #[arg(short, long)]
    state: Option<String>,

    /// V2 only: set channel frequency (1-16)
    #[arg(short = 'c', long)]
    channel: Option<u8>,

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

    let action = match (&args.state, args.channel) {
        (Some(state), _) => Action::State(State::from_str(state)?),
        (None, Some(channel)) if matches!(channel, 1..=16) => Action::Channel(channel),
        (None, Some(channel)) => {
            return Err(Error::InvalidChannel(format!(
                "V2 channels are 1-16, got {channel}"
            )))
        }
        (None, None) => return show_info(&args).await,
    };

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

            if let Some(desc) = process_peripheral(
                adapter,
                peripheral,
                &action,
                &args.bsid,
                args.retries.max(1),
                Duration::from_secs(args.retry_delay),
            )
            .await?
            {
                info!("{desc}");
            }
        }
    }

    Ok(())
}

/// Shows status and channel information for discovered base stations.
async fn show_info(args: &Args) -> Result<(), Error> {
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
            |peripherals| all_requested_targets_found(peripherals, &args.bsid),
        )
        .await?;

        if peripherals.is_empty() {
            return Err(Error::Message(String::from(
                "->>> BLE peripheral devices were not found. Exiting...",
            )));
        }

        for peripheral in &peripherals {
            let peripheral_id_str = peripheral.id.to_string();
            let version = BaseStationVersion::detect(&peripheral.name);
            if version == BaseStationVersion::Unknown {
                continue;
            }

            // When a BSID filter is given, only show matching devices
            if !args.bsid.is_empty()
                && version
                    .matches_bsid(&peripheral.name, &peripheral_id_str, &args.bsid)
                    .is_none()
            {
                continue;
            }

            match version {
                BaseStationVersion::V2 => {
                    let status = match lighthouse::v2_status(adapter, &peripheral.id).await {
                        Ok(status) => status,
                        Err(error) => {
                            warn!(
                                "{} [{}]: failed to read status: {error}",
                                peripheral.name, peripheral_id_str
                            );
                            continue;
                        }
                    };

                    let power = status
                        .power_state
                        .map_or_else(|| String::from("N/A"), lighthouse::v2_power_state_name);
                    let channel = status
                        .channel
                        .map_or_else(|| String::from("N/A"), lighthouse::v2_channel_name);

                    info!(
                        "{} [{}]: {power}, Channel {channel}",
                        peripheral.name, peripheral_id_str
                    );
                }
                BaseStationVersion::V1 | BaseStationVersion::Unknown => info!(
                    "{} [{}]: V1 (status and channel not supported)",
                    peripheral.name, peripheral_id_str
                ),
            }
        }
    }

    Ok(())
}
