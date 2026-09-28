use socketcan::{
    CanFilter, CanFrame, CanSocket, EmbeddedFrame, Frame, Socket, SocketOptions, StandardId,
};
use std::time::Duration;

use crate::francor::franklyboot::{
    com::{
        msg::{Msg, RequestType},
        ComConnParams, ComInterface, ComMode,
    },
    Error,
};

// CAN Interface ----------------------------------------------------------------------------------

pub const CAN_BASE_ID: u32 = 0x781;
pub const CAN_BROADCAST_ID: u32 = 0x780;
pub const CAN_MAX_ID: u32 = 0x7FF;
pub const CAN_RX_TIMEOUT: std::time::Duration = Duration::from_millis(500);

///
/// CAN interface
///
/// This struct implements the communication interface for can communication.
///
pub struct CANInterface {
    /// CAN socket
    socket: Option<CanSocket>,

    /// Timeout for receiving messages
    timeout: Duration,

    // Broadcast or specific node communication?
    mode: ComMode,
}

impl CANInterface {
    // Private functions --------------------------------------------------------------------------

    fn can_frame_to_msg(can_frame: &socketcan::frame::CanDataFrame) -> Msg {
        let data = can_frame.data();
        let msg_data = [
            data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
        ];

        Msg::from_raw_data_array(&msg_data)
    }
}

impl ComInterface for CANInterface {
    fn create() -> Result<Self, Error> {
        Ok(CANInterface {
            socket: None,
            timeout: CAN_RX_TIMEOUT,
            mode: ComMode::Broadcast,
        })
    }

    fn open(&mut self, params: &ComConnParams) -> Result<(), Error> {
        if params.name.is_none() {
            return Err(Error::Error("Serial port name not set!".to_string()));
        }

        let socket = CanSocket::open(params.name.clone().unwrap().as_str());
        match socket {
            Ok(socket) => {
                socket
                    .set_read_timeout(Some(self.timeout))
                    .map_err(|_e| Error::Error("Failed to set rx timeout!".to_string()))?;

                // \todo extract filtering into own function, because it is also used in set_mode()

                // Set ID and Mask to receive answers only from bootloader nodes
                let can_rx_msg_id = CAN_BROADCAST_ID;
                let can_rx_msg_mask = 0x7F0;
                // Set filter
                let filter = CanFilter::new(can_rx_msg_id, can_rx_msg_mask);

                socket
                    .set_filters(&[filter])
                    .map_err(|e| Error::Error(format!("Failed to set filter: {}", e)))?;

                // clear rx messages
                while socket.read_frame().is_ok() {}

                self.socket = Some(socket);

                Ok(())
            }
            Err(e) => Err(Error::Error(format!(
                "Error opening socket for \"{}\": \"{}\"",
                params.name.clone().unwrap(),
                e
            ))),
        }
    }

    fn is_network() -> bool {
        true
    }

    // \todo: current behaviour: scan network fails with error if the first node doesnt respond. expected behaviour: scan network should still wait for other nodes responses
    fn scan_network(&mut self) -> Result<Vec<u8>, Error> {
        // Config interface to broadcast
        self.set_mode(ComMode::Broadcast)?;

        // Send ping
        let ping_request = Msg::new_std_request(RequestType::Ping);
        self.send(&ping_request)?;

        match self.socket.as_mut() {
            Some(socket) => {
                // Receive until no new response
                // Store node ids
                let mut node_id_lst = Vec::new();
                while let Ok(frame) = socket.read_frame() {
                    // Only process data frames
                    if let CanFrame::Data(can_frame) = frame {
                        let response = Self::can_frame_to_msg(&can_frame);
                        if ping_request.is_response_ok(&response).is_ok() {
                            let raw_id = can_frame.raw_id();
                            let node_id = ((raw_id - CAN_BASE_ID) / 2) as u8;
                            node_id_lst.push(node_id);
                        }
                    }
                }

                Ok(node_id_lst)
            }
            None => Err(Error::Error("CAN socket not open!".to_string())),
        }
    }

    fn set_mode(&mut self, mode: ComMode) -> Result<(), Error> {
        self.mode = mode;
        match self.socket.as_mut() {
            Some(socket) => {
                // Set ID and Mask dependent on the mode
                let (can_rx_msg_id, can_rx_msg_mask) = match mode {
                    ComMode::Broadcast => (CAN_BROADCAST_ID, 0x7F0),
                    ComMode::Specific(node_id) => (CAN_BASE_ID + node_id as u32 * 2 + 1, 0x7FF),
                };
                // Set filter
                let filter = CanFilter::new(can_rx_msg_id, can_rx_msg_mask);
                match socket.set_filters(&[filter]) {
                    Ok(_) => Ok(()),
                    Err(e) => Err(Error::Error(format!("Failed to set filter: {}", e))),
                }
            }
            None => Err(Error::Error("CAN socket not open!".to_string())),
        }
    }

    fn set_timeout(&mut self, timeout: std::time::Duration) -> Result<(), Error> {
        match self.socket.as_mut() {
            Some(socket) => match socket.set_read_timeout(Some(timeout)) {
                Ok(_) => {
                    self.timeout = timeout;

                    Ok(())
                }
                Err(e) => Err(Error::Error(format!("Failed to set timeout: {}", e))),
            },
            None => Err(Error::Error("CAN socket not open!".to_string())),
        }
    }

    fn get_timeout(&self) -> std::time::Duration {
        self.timeout
    }

    fn send(&mut self, msg: &Msg) -> Result<(), Error> {
        match self.socket.as_mut() {
            Some(socket) => {
                let id = match self.mode {
                    ComMode::Broadcast => StandardId::new(CAN_BROADCAST_ID as u16),
                    ComMode::Specific(node_id) => {
                        StandardId::new(CAN_BASE_ID as u16 + node_id as u16 * 2)
                    }
                }
                .ok_or_else(|| Error::Error("Invalid CAN ID".to_string()))?;
                let frame = socketcan::frame::CanDataFrame::new(id, &msg.to_raw_data_array())
                    .ok_or_else(|| Error::Error("Failed to create CAN data frame".to_string()))?;

                socket
                    .write_frame(&frame)
                    .map_err(|e| Error::Error(format!("{}", e)))?;

                Ok(())
            }
            None => Err(Error::Error("CAN socket not open!".to_string())),
        }
    }

    fn recv(&mut self) -> Result<Msg, Error> {
        match self.socket.as_mut() {
            Some(socket) => {
                if let Ok(frame) = socket.read_frame() {
                    // Only process data frames
                    if let CanFrame::Data(can_frame) = frame {
                        return Ok(Self::can_frame_to_msg(&can_frame));
                    }
                    // Non-data frames are ignored, try again
                    return Err(Error::ComNoResponse);
                }

                Err(Error::ComNoResponse)
            }
            None => Err(Error::Error("CAN socket not open!".to_string())),
        }
    }
}
