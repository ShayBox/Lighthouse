<div align="center">
  <a href="https://discord.shaybox.com">
    <img alt="Discord" src="https://img.shields.io/discord/824865729445888041?color=404eed&label=Discord&logo=Discord&logoColor=FFFFFF">
  </a>
  <a href="https://github.com/shaybox/lighthouse/releases/latest">
    <img alt="Downloads" src="https://img.shields.io/github/downloads/shaybox/lighthouse/total?color=3fb950&label=Downloads&logo=github&logoColor=FFFFFF">
  </a>
</div>

# Lighthouse
Virtual reality basestation power management in Rust

## Usage

```
Usage: lighthouse [OPTIONS] --state <STATE>

Options:
  -s, --state <STATE>     V1: [OFF|ON] [BSID] | V2: [OFF|ON|STANDBY] (BSID)
  -b, --bsid <BSID>       Basestation BSID
  -v, --verbose...        More output per occurrence
  -q, --quiet...          Less output per occurrence
  -h, --help              Print help information
  -t, --timeout <SECONDS> Scan timeout
```
V1 Basestations require an 8 character BSID found on the device to work.

V2 Basestations do not require BSID. But you can specify their MAC address as BSID to manage a specific device.

## Example
V1: `$ lighthouse -s on -b aabbccdd`  
V2: `$ lighthouse -s on` or `$ lighthouse -s on -b A1:B2:C3:D4:E5:F6`  

## macOS
Enable the Bluetooth permission for your terminal. You can do the latter by going to System Preferences → Security & Privacy → Privacy → Bluetooth, clicking the '+' button, and selecting 'Terminal' (or iTerm or whichever terminal application you use).
