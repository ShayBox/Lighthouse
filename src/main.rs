use btleplug::api::{Central, Peripheral, WriteType};
#[cfg(target_os = "linux")]
use btleplug::bluez::manager::Manager;
#[cfg(target_os = "windows")]
use btleplug::winrtble::{adapter::Adapter, manager::Manager};
use std::{env, process::exit, thread, time::Duration};
use uuid::Uuid;

pub fn main() {
    let args: Vec<String> = env::args().collect();

    let cmd;
    if args.len() > 1 {
        if args[1] == "off" {
            cmd = vec![0x00];
        } else if args[1] == "on" {
            cmd = vec![0x01];
        } else if args[1] == "standby" {
            cmd = vec![0x02];
        } else {
            println!("Valid states: off, on, standby");
            exit(128);
        }
    } else {
        println!("Please provide a state: off, on, standby");
        exit(128);
    }

    let characteristic_uuid = Uuid::parse_str("00001525-1212-efde-1523-785feabcd124").unwrap();
    let manager = Manager::new().unwrap();
    let adapters = manager.adapters().unwrap();
    let central = adapters.into_iter().nth(0).unwrap();

    central.start_scan().unwrap();
    thread::sleep(Duration::from_secs(5));

    let lighthouses = central.peripherals().into_iter().filter(|p| {
        p.properties()
            .local_name
            .iter()
            .any(|name| name.contains("LHB-"))
    });

    for lighthouse in lighthouses {
        match lighthouse.connect() {
            Ok(_) => {}
            Err(_) => lighthouse.disconnect().unwrap(),
        };
        match lighthouse.discover_characteristics() {
            Ok(_) => {}
            Err(_) => lighthouse.disconnect().unwrap(),
        };

        let chars = lighthouse.characteristics();
        let char = chars
            .iter()
            .find(|c| c.uuid == characteristic_uuid)
            .unwrap_or_else(|| {
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
}
