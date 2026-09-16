use lighthouse::{
    Action, BaseStationVersion, Error, State, V2_MODE_UUID, V2_POWER_STATE_UUID,
    v2_channel_command, v2_channel_name, v2_power_state_name,
};

#[test]
fn test_v2_channel_command_valid() {
    assert_eq!(v2_channel_command(1).unwrap(), vec![0x01]);
    assert_eq!(v2_channel_command(8).unwrap(), vec![0x08]);
    assert_eq!(v2_channel_command(15).unwrap(), vec![0x0F]);
    assert_eq!(v2_channel_command(16).unwrap(), vec![0x10]);
}

#[test]
fn test_v2_channel_command_invalid() {
    assert!(matches!(
        v2_channel_command(0),
        Err(Error::InvalidChannel(_))
    ));
    assert!(matches!(
        v2_channel_command(17),
        Err(Error::InvalidChannel(_))
    ));
}

#[test]
fn test_v2_action_command() {
    let (cmd, uuid) = BaseStationVersion::V2.command(&Action::Channel(5), None).unwrap();
    assert_eq!(cmd, vec![0x05]);
    assert_eq!(uuid, *V2_MODE_UUID);

    let (cmd, uuid) = BaseStationVersion::V2
        .command(&Action::State(State::On), None)
        .unwrap();
    assert_eq!(cmd, vec![0x01]);
    assert_eq!(uuid, *V2_POWER_STATE_UUID);
}

#[test]
fn test_v1_action_command_rejects_channel() {
    let result = BaseStationVersion::V1.command(&Action::Channel(5), Some("aabbccdd"));
    assert!(result.is_err());
}

#[test]
fn test_power_state_names() {
    assert_eq!(v2_power_state_name(0x00), "OFF");
    assert_eq!(v2_power_state_name(0x02), "STANDBY");
    assert_eq!(v2_power_state_name(0x01), "ON");
    assert_eq!(v2_power_state_name(0x09), "ON");
    assert_eq!(v2_power_state_name(0x0B), "ON");
    assert_eq!(v2_power_state_name(0xFF), "UNKNOWN(0xFF)");
}

#[test]
fn test_channel_names() {
    assert_eq!(v2_channel_name(1), "1");
    assert_eq!(v2_channel_name(16), "16");
    assert_eq!(v2_channel_name(0), "UNKNOWN(0x00)");
    assert_eq!(v2_channel_name(0xFF), "UNKNOWN(0xFF)");
}

#[test]
fn test_action_display() {
    assert_eq!(Action::State(State::On).to_string(), "ON");
    assert_eq!(Action::Channel(5).to_string(), "CHANNEL 5");
}
