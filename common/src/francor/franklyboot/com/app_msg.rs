//! # Application Messages
//!
//! For easier access to and usage of the bootloader, devs are encouraged to adapt their application firmware
//! to respond to the following messages from this toolset.
//! These messages are **not** part of the bootloader protocol (`msg.rs`), the bootloader itsself does not send or expect them.
//! They are optional, however implementing them in your application firmware will make your life easier.
//!
//! Absolutely make sure that new application messages do not interfere with bootloaders already running on the bus!
//!
//! ## Communication Interfaces
//!
//! - **Serial**: (Not yet implemented)
//! - **CAN**: CAN bus multi-device network (e.g., can0, vcan0)
//! - **SIM**: (Not yet implemented)
//! - **Ethernet**: (Not yet implemented)

// Wake-up call
// is sent to the application, in order to trigger a restart in boodloader mode.

/// Command byte of the wake-up call
pub const WAKEUP_REQ: u16 = 0xA000;

/// Command byte of the acknowledge
pub const WAKEUP_ACK: u16 = 0xA001;

/// Payload of the wake-up call frame
pub fn wakeup_payload() -> [u8; 2] {
    [WAKEUP_REQ as u8, (WAKEUP_REQ >> 8) as u8]
}

/// Checks if a received frame is a wake-up acknowledge
pub fn is_wakeup_ack(data: &[u8]) -> bool {
    data.len() == 2 && u16::from_le_bytes([data[0], data[1]]) == WAKEUP_ACK
}
