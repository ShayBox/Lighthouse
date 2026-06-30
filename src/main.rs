use std::time::Duration;

use clap::Parser;
use clap_verbosity_flag::Verbosity;
use lighthouse::Error;
use tracing::{info, warn};
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
    #[arg(short, long, value_delimiter = ',', num_args = 1..)]
    bsid: Vec<String>,

    #[clap(flatten)]
    verbose: Verbosity,

    /// Request timeout in seconds
    #[arg(short, long, default_value_t = 10)]
    timeout: u64,

    /// Number of write attempts per basestation
    #[arg(long, default_value_t = 3)]
    retries: u32,

    /// Delay between write attempts in seconds
    #[arg(long, default_value_t = 2)]
    retry_delay: u64,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Error> {
    let args = Args::parse();
    let state = args.state.to_uppercase();
    let normalized_bsid_inputs = args
        .bsid
        .iter()
        .map(|input| input.to_lowercase().replace('_', ":"))
        .collect::<Vec<_>>();

    tracing_subscriber::fmt()
        .with_max_level(args.verbose.log_level_filter().as_trace())
        .init();

    let v1_uuid = Uuid::parse_str(V1_UUID).map_err(Error::Uuid)?;
    let v2_uuid = Uuid::parse_str(V2_UUID).map_err(Error::Uuid)?;

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
                all_requested_targets_found(peripherals, &args.bsid, &normalized_bsid_inputs)
            },
        )
        .await?;
        if peripherals.is_empty() {
            return Err(Error::Message(String::from(
                "->>> BLE peripheral devices were not found. Exiting...",
            )));
        }

        let mut failures = Vec::new();
        for peripheral in &peripherals {
            let peripheral_id_str = peripheral.id.to_string();
            info!("Found '{}' [{}]", peripheral.name, peripheral_id_str);

            if peripheral.name.starts_with("LHB-") {
                // V2
                if !matches_v2_bsid(&peripheral_id_str, optional_inputs(&normalized_bsid_inputs)) {
                    continue;
                }
                let cmd = v2_cmd(&state)?;
                if let Err(error) = write_with_retries(
                    adapter,
                    peripheral,
                    &cmd,
                    v2_uuid,
                    args.retries.max(1),
                    Duration::from_secs(args.retry_delay),
                )
                .await
                {
                    failures.push(format!(
                        "{} [{}]: {error}",
                        peripheral.name, peripheral_id_str
                    ));
                    continue;
                }
            } else if let Some(bsid) = matches_v1_bsid(&peripheral.name, &args.bsid) {
                // v1
                let cmd = v1_cmd(&state, bsid)?;
                if let Err(error) = write_with_retries(
                    adapter,
                    peripheral,
                    &cmd,
                    v1_uuid,
                    args.retries.max(1),
                    Duration::from_secs(args.retry_delay),
                )
                .await
                {
                    failures.push(format!(
                        "{} [{}]: {error}",
                        peripheral.name, peripheral_id_str
                    ));
                    continue;
                }
            } else {
                continue;
            } // not supported
            info!("{} [{}]: {}", peripheral.name, peripheral_id_str, state);
        }

        if !failures.is_empty() {
            return Err(Error::Message(format!(
                "Failed to update basestation(s): {}",
                failures.join("; ")
            )));
        }
    }
    Ok(())
}

async fn write_with_retries(
    adapter: &btleplug::platform::Adapter,
    peripheral: &lighthouse::DiscoveredPeripheral,
    cmd: &[u8],
    uuid: Uuid,
    retries: u32,
    retry_delay: Duration,
) -> Result<(), Error> {
    for attempt in 1..=retries {
        match lighthouse::write(adapter, &peripheral.id, cmd, uuid).await {
            Ok(()) => return Ok(()),
            Err(error) if attempt == retries => return Err(error),
            Err(error) => {
                warn!(
                    "Attempt {attempt}/{retries} failed for '{}' [{}]: {error}",
                    peripheral.name, peripheral.id
                );
                tokio::time::sleep(retry_delay).await;
            }
        }
    }

    Err(Error::Message(String::from("No write attempts were made")))
}

fn optional_inputs(inputs: &[String]) -> Option<&[String]> {
    (!inputs.is_empty()).then_some(inputs)
}

fn all_requested_targets_found(
    peripherals: &[lighthouse::DiscoveredPeripheral],
    bsids: &[String],
    normalized_bsid_inputs: &[String],
) -> bool {
    if bsids.is_empty() {
        return false;
    }

    bsids
        .iter()
        .zip(normalized_bsid_inputs)
        .all(|(bsid, normalized_bsid)| {
            peripherals.iter().any(|peripheral| {
                let peripheral_id_str = peripheral.id.to_string();
                if peripheral.name.starts_with("LHB-")
                    && matches_v2_bsid(
                        &peripheral_id_str,
                        Some(std::slice::from_ref(normalized_bsid)),
                    )
                {
                    return true;
                }

                matches_v1_bsid(&peripheral.name, std::slice::from_ref(bsid)).is_some()
            })
        })
}

fn matches_v2_bsid(peripheral_id: &str, normalized_inputs: Option<&[String]>) -> bool {
    let Some(normalized_inputs) = normalized_inputs else {
        return true;
    };

    // On Linux systems the peripheral ID will be something like "hci0/dev_A1_B2_C3_D4_E5_F6"
    // instead of "A1:B2:C3:D4:E5:F6". Normalize the strings to allow user input in either format.
    let normalized_peripheral_id = peripheral_id.to_lowercase().replace('_', ":");
    normalized_inputs
        .iter()
        .any(|input| normalized_peripheral_id.contains(input.as_str()))
}

fn matches_v1_bsid<'a>(name: &str, bsids: &'a [String]) -> Option<&'a str> {
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

fn v2_cmd(state: &str) -> Result<Vec<u8>, Error> {
    match state {
        "OFF" => Ok(vec![0x00]),
        "ON" => Ok(vec![0x01]),
        "STANDBY" => Ok(vec![0x02]),
        _ => Err(Error::Message(format!(
            "V2: Unknown State {state}, Available: [OFF|ON|STANDBY]"
        ))),
    }
}

fn v1_cmd(state: &str, bsid: &str) -> Result<Vec<u8>, Error> {
    let aa = u8::from_str_radix(&bsid[0..2], 16).map_err(Error::Std)?;
    let bb = u8::from_str_radix(&bsid[2..4], 16).map_err(Error::Std)?;
    let cc = u8::from_str_radix(&bsid[4..6], 16).map_err(Error::Std)?;
    let dd = u8::from_str_radix(&bsid[6..8], 16).map_err(Error::Std)?;

    match state {
        "OFF" | "STANDBY" => Ok(vec![
            0x12, 0x02, 0x00, 0x01, dd, cc, bb, aa, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
        ]),
        "ON" => Ok(vec![
            0x12, 0x00, 0x00, 0x00, dd, cc, bb, aa, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
        ]),
        _ => Err(Error::Message(format!(
            "V1: Unknown State {state}, Available: [OFF|ON]"
        ))),
    }
}
