//! # Application Messages
//!
//! For easier access to and usage of the bootloader, devs are encouraged to adapt their application firmware
//! to react to the following messages from this toolset.
//! These messages are **not** part of the bootloader protocol (`msg.rs`), they are set to a length of 7 Bytes so bootloaders do not react to them.
//! Application messages are optional, however implementing them in your application firmware will make your life easier.
//!
//!
//! ## Communication Interfaces
//!
//! - **Serial**: (Not yet implemented)
//! - **CAN**: CAN bus multi-device network (e.g., can0, vcan0)
//! - **SIM**: (Not yet implemented)
//! - **Ethernet**: (Not yet implemented)

use crate::francor::franklyboot::Error;

pub const APP_MSG_LEN: usize = 7;
pub const APP_PAYLOAD_LEN: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppMsg {
    pub message_type: u16,
    pub payload: [u8; APP_PAYLOAD_LEN],
}

impl AppMsg {
    pub fn new(message_type: u16, payload: [u8; APP_PAYLOAD_LEN]) -> Self {
        Self {
            message_type,
            payload,
        }
    }

    pub fn request(request_type: u16) -> Self {
        Self::new(request_type, [0; APP_PAYLOAD_LEN])
    }

    pub fn from_raw_data_array(frame: &[u8]) -> Result<Self, Error> {
        if frame.len() != APP_MSG_LEN {
            return Err(Error::MsgCorruption(format!(
                "Application message must be {} bytes, received {}",
                APP_MSG_LEN,
                frame.len()
            )));
        }

        Ok(Self {
            message_type: u16::from_be_bytes([frame[0], frame[1]]),
            payload: [frame[2], frame[3], frame[4], frame[5], frame[6]],
        })
    }

    pub fn to_raw_data_array(self) -> [u8; APP_MSG_LEN] {
        let [type_high, type_low] = self.message_type.to_be_bytes();
        [
            type_low,
            type_high,
            self.payload[0],
            self.payload[1],
            self.payload[2],
            self.payload[3],
            self.payload[4],
        ]
    }

    pub fn validate_response(
        self,
        expected_response_type: u16,
    ) -> Result<[u8; APP_PAYLOAD_LEN], Error> {
        if self.message_type != expected_response_type {
            return Err(Error::MsgCorruption(format!(
                "Application response type mismatch: expected {:#06X}, received {:#06X}",
                expected_response_type, self.message_type
            )));
        }

        Ok(self.payload)
    }
}

pub fn process_response(
    frame: &[u8],
    expected_response_type: u16,
) -> Result<[u8; APP_PAYLOAD_LEN], Error> {
    AppMsg::from_raw_data_array(frame)?.validate_response(expected_response_type)
}

// Wake-up call
// is sent to the application, in order to trigger a restart in boodloader mode.
pub const WAKEUP_REQ: u16 = 0xA000;

// Get firmware version
// is sent to the application, in order to read out the firmware version of the application.
pub const FIRMWARE_VERSION_REQ: u16 = 0xA001;
pub const FIRMWARE_VERSION_RES: u16 = 0xA101;

// Get Product ID
// is sent to the application, in order to read out the product id of the application.
pub const PRODUCT_ID_REQ: u16 = 0xA002;
pub const PRODUCT_ID_RES: u16 = 0xA102;
