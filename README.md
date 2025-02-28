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
  -s, --state <STATE>      V1: [OFF|ON] | V2: [OFF|ON|STANDBY]
  -b, --bsid <BSID>        V1: Basestation BSID (Required) | V2: Bluetooth Device Identifier (Optional)
  -v, --verbose...         Increase logging verbosity
  -q, --quiet...           Decrease logging verbosity
  -t, --timeout <TIMEOUT>  Request timeout in seconds [default: 10]
  -h, --help               Print help
```
V1 Basestations require an 8 character BSID found on the device to work.

V2 Basestations do not require BSID. But you can specify their MAC address as BSID to manage a specific device.

### Examples

**Turning a V1 lighthouse on:**

Find the BSID at the back of the device.

```bash
$ lighthouse --state on --bsid aabbccdd
```  
**Turning on any V2 lighthouses within range:**

```bash
$ lighthouse --state on
```

**Turning on a specific V2 lighthouse:**

Run once with verbose parameters to find the MAC address for each lighthouse:
```bash
$ lighthouse -vv --state off
``` 

This will show the device path or MAC address within square brackets, looking something like this:
```
2025-02-28T22:14:58.528048Z  INFO lighthouse: Found 'LHB-6DC32F38' [hci0/dev_E2_5A_B0_E4_97_AD]
2025-02-28T22:15:33.543205Z  INFO lighthouse: LHB-6DC32F38 [hci0/dev_E2_5A_B0_E4_97_AD]: OFF
```

Use the ID shown in the square brackets in the previous command as the bsid to manage a specific lighthouse:
```bash
$ lighthouse --state on --bsid "hci0/dev_E2_5A_B0_E4_97_AD"
# or
$ lighthouse --state on --bsid "E2:5A:B0:E4:97:AD"
```

## macOS
Enable the Bluetooth permission for your terminal. You can do the latter by going to System Preferences → Security & Privacy → Privacy → Bluetooth, clicking the '+' button, and selecting 'Terminal' (or iTerm or whichever terminal application you use).
