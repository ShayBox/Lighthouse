use btleplug::api::{BDAddr, Central, Peripheral, WriteType};
#[cfg(target_os = "linux")]
use btleplug::bluez::{adapter::Adapter, manager::Manager};
#[cfg(target_os = "windows")]
use btleplug::winrtble::{adapter::Adapter, manager::Manager};
use std::{env, process::exit, thread, time::Duration, vec::Vec};
use uuid::Uuid;

pub fn main() {
    let args: Vec<String> = env::args().collect();

    let mut cmd;
    if args.len() > 1 {
        if args[1] == "off" {
            cmd = vec![0x00];
        } else if args[1] == "on" {
            cmd = vec![0x01];
        } else if args[1] == "standby" {
            cmd = vec![0x02];
        } else {
            println!("V2: Valid states: on, off, standby");
            exit(128);
        }
    } else {
        println!("Lighthouse - VR Lighthouse power state management in Rust");
        println!("");
        println!("lighthouse [on | off] [v1 ID]");
        println!("lighthouse [on | off | standby]");
        exit(128);
    }

    let manager = Manager::new().unwrap();
    let adapters = manager.adapters().unwrap();
    let central = adapters.into_iter().nth(0).unwrap();

    central.start_scan().unwrap();
    thread::sleep(Duration::from_secs(5));

    if args.len() > 2 {
        let lighthouses_v1 = central.peripherals().into_iter().filter(|p| {
            p.properties().local_name.iter().any(|name| {
                name.starts_with("HTC BS")
                    && name[(name.len() - 4)..4] == args[2][(args[2].len() - 4)..4]
            })
        });

        for lighthouse in lighthouses_v1 {
            let name = match lighthouse.properties().local_name {
                Some(s) => s,
                None => exit(1),
            };

            let dd = name[(name.len() - 2)..4].parse().unwrap();
            let cc = name[(name.len() - 4)..2].parse().unwrap();
            let bb = args[2][2..4].parse().unwrap();
            let aa = args[2][0..2].parse().unwrap();

            if args[1] == "off" {
                cmd = vec![
                    0x12, 0x02, 0x00, 0x01, dd, cc, bb, aa, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                ];
            } else if args[1] == "on" {
                cmd = vec![
                    0x12, 0x00, 0x00, 0x00, dd, cc, bb, aa, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                ];
            } else {
                println!("V1: Valid states: on, off");
                exit(128);
            }

            let c_uuid = Uuid::parse_str("0000cb01-0000-1000-8000-00805f9b34fb").unwrap();
            write_lighthouse(&central, lighthouse.address(), &cmd, c_uuid)
        }
    } else {
        let lighthouses_v2 = central.peripherals().into_iter().filter(|p| {
            p.properties()
                .local_name
                .iter()
                .any(|name| name.contains("LHB-"))
        });

        for lighthouse in lighthouses_v2 {
            let c_uuid = Uuid::parse_str("00001525-1212-efde-1523-785feabcd124").unwrap();
            write_lighthouse(&central, lighthouse.address(), &cmd, c_uuid)
        }
    }
}

pub fn write_lighthouse(central: &Adapter, address: BDAddr, cmd: &[u8], c_uuid: Uuid) {
    let lighthouse = match central.peripheral(address) {
        Some(x) => x,
        None => exit(1),
    };

    match lighthouse.connect() {
        Ok(_) => {}
        Err(_) => lighthouse.disconnect().unwrap(),
    };
    match lighthouse.discover_characteristics() {
        Ok(_) => {}
        Err(_) => lighthouse.disconnect().unwrap(),
    };

    let chars = lighthouse.characteristics();
    let char = chars.iter().find(|c| c.uuid == c_uuid).unwrap_or_else(|| {
        lighthouse.disconnect().unwrap();
        exit(1)
    });

    match lighthouse.write(&char, &cmd, WriteType::WithoutResponse) {
        Ok(_) => {}
        Err(_) => lighthouse.disconnect().unwrap(),
    };

    thread::sleep(Duration::from_millis(200));

    lighthouse.disconnect().unwrap();
}
