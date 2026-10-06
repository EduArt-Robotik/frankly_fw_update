use crate::francor::franklyboot::{
    com::{msg::MsgData, msg::RequestType, ComInterface},
    device::{Entry, EntryList, EntryType},
    firmware::{AppFirmware, FirmwareDataInterface},
    flash::FlashDesc,
    Error, ProgressUpdate,
};
use std::fmt;

// Device -----------------------------------------------------------------------------------------

///
/// Device Representationg
///
/// This struct represents the device. It contains all information about the device and provides
/// functions to read and write data from and to the device.
///
pub struct Device<I> {
    // Interface
    interface: I,

    // Flash description
    flash_desc: FlashDesc,

    /// Vector of all entries
    entries: EntryList,

    /// Optional progress callback for operation updates
    progress_fn: Option<Box<dyn Fn(ProgressUpdate) + Send>>,
}

/// Implementation of the Display trait for the Device struct
impl<T> fmt::Display for Device<T>
where
    T: ComInterface,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "VID: {:#010X} | PID: {:#010X} | PRD: {:#010X} | UID: {:#010X}",
            self.entries.get_entry_value(RequestType::DevInfoVID),
            self.entries.get_entry_value(RequestType::DevInfoPID),
            self.entries.get_entry_value(RequestType::DevInfoPRD),
            self.get_device_info_uid()
        )
    }
}

impl<I> Device<I>
where
    I: ComInterface,
{
    /// Create a new device
    pub fn new(interface: I) -> Self {
        Self::new_with_progress(interface, None)
    }

    /// Create a new device with a custom progress callback
    ///
    /// The progress callback will be called for progress updates during operations like
    /// erase, flash, and reset. If None is provided, no progress reporting will occur.
    pub fn new_with_progress(
        interface: I,
        progress_fn: Option<Box<dyn Fn(ProgressUpdate) + Send>>,
    ) -> Self {
        let mut device = Self {
            interface,
            flash_desc: FlashDesc::new(0, 0, 0),
            entries: EntryList::new(),
            progress_fn,
        };

        device._add_entry(EntryType::Const, RequestType::DevInfoBootloaderVersion);
        device._add_entry(EntryType::Const, RequestType::DevInfoBootloaderCRC);
        device._add_entry(EntryType::Const, RequestType::DevInfoVID);
        device._add_entry(EntryType::Const, RequestType::DevInfoPID);
        device._add_entry(EntryType::Const, RequestType::DevInfoPRD);
        device._add_entry(EntryType::Const, RequestType::DevInfoUID1);
        device._add_entry(EntryType::Const, RequestType::DevInfoUID2);
        device._add_entry(EntryType::Const, RequestType::DevInfoUID3);
        device._add_entry(EntryType::Const, RequestType::DevInfoUID4);

        device._add_entry(EntryType::Const, RequestType::FlashInfoStartAddr);
        device._add_entry(EntryType::Const, RequestType::FlashInfoPageSize);
        device._add_entry(EntryType::Const, RequestType::FlashInfoNumPages);

        device._add_entry(EntryType::Const, RequestType::AppInfoPageIdx);
        device._add_entry(EntryType::RO, RequestType::AppInfoCRCCalc);

        device._add_entry(EntryType::Cmd, RequestType::PageBufferClear);
        device._add_entry(EntryType::RW, RequestType::PageBufferWriteWord);
        device._add_entry(EntryType::RO, RequestType::PageBufferCalcCRC);
        device._add_entry(EntryType::Cmd, RequestType::PageBufferWriteToFlash);

        device._add_entry(EntryType::Cmd, RequestType::FlashWriteErasePage);
        device._add_entry(EntryType::Cmd, RequestType::FlashWriteAppCRC);

        device._add_entry(EntryType::Cmd, RequestType::StartApp);
        device._add_entry(EntryType::Cmd, RequestType::ResetDevice);

        device
    }

    /// Initialize the device struct
    ///
    /// This function reads all constant data from the device and stores it in the device struct.
    pub fn init(&mut self) -> Result<(), Error> {
        // Read constant data from device
        self._read_const_data()?;

        // Get complete flash description
        let flash_start = self.get_entry_value(RequestType::FlashInfoStartAddr);
        let flash_page_size = self.get_entry_value(RequestType::FlashInfoPageSize);
        let flash_num_pages = self.get_entry_value(RequestType::FlashInfoNumPages);
        let flash_size = flash_page_size * flash_num_pages;
        let flash_app_page_idx = self.get_entry_value(RequestType::AppInfoPageIdx);

        // Calculate bootloader area
        let bootloader_start = flash_start;
        let bootloader_size = flash_app_page_idx * flash_page_size;

        // Calculate application area
        let app_start = flash_start + (flash_app_page_idx * flash_page_size);
        let app_size = flash_size - bootloader_size;

        // Create flash description
        self.flash_desc = FlashDesc::new(flash_start, flash_size, flash_page_size);

        // Add bootloader section
        self.flash_desc
            .add_section("Bootloader", bootloader_start, bootloader_size)
            .map_err(|e| Error::Error(format!("Failed to add bootloader section: {}", e)))?;

        // Add application section
        self.flash_desc
            .add_section("Application", app_start, app_size)
            .map_err(|e| Error::Error(format!("Failed to add application section: {}", e)))?;

        Ok(())
    }

    /// Reset the device
    ///
    /// This function resets the device.
    ///
    pub fn reset(&mut self) -> Result<(), Error> {
        self.entries
            .get_entry_mut(RequestType::ResetDevice)
            .exec(&mut self.interface, 0)?;

        self.progress(ProgressUpdate::Message("Reset device...".to_string()));

        Ok(())
    }

    /// Restart the firmware of the device
    ///
    /// This function restarts the firmware of the device.
    ///
    pub fn start_app(&mut self) -> Result<(), Error> {
        self.entries
            .get_entry_mut(RequestType::StartApp)
            .exec(&mut self.interface, 0)?;

        self.progress(ProgressUpdate::Message("Starting application...".to_string()));

        Ok(())
    }

    /// Erase the application area
    ///
    /// This function erases the application area of the device.
    ///
    pub fn erase(&mut self) -> Result<(), Error> {
        let app_section = self.flash_desc.get_section("Application").unwrap();
        let page_range: Vec<u32> = app_section.get_page_range().collect();
        let total_pages = page_range.len() as u32;

        for (idx, flash_page_id) in page_range.iter().enumerate() {
            let current = (idx + 1) as u32;
            self.progress(ProgressUpdate::EraseProgress {
                current,
                total: total_pages,
            });

            // Erase flash page
            self.entries
                .get_entry_mut(RequestType::FlashWriteErasePage)
                .exec(&mut self.interface, *flash_page_id)?;
        }

        Ok(())
    }

    /// Flash a new firmware to the device
    ///
    /// This function flashes a new firmware to the device. It reads the firmware data from the
    /// FirmwareDataInterface and writes it to the device.
    ///
    pub fn flash<FWI: FirmwareDataInterface>(&mut self, fwi: &FWI) -> Result<(), Error> {
        // Read necessary data to variables
        let app_section = self.flash_desc.get_section("Application").unwrap();

        let fw_data = fwi.get_firmware_data().unwrap();
        let fw_size = fw_data.len() as u32;
        let fw_num_pages = (fw_size / app_section.get_page_size()) + 1;

        // Print firmware information
        self.progress(ProgressUpdate::Message(format!(
            "Firmware Data: Size: {:#.2} kB Num Pages: {}",
            (fw_size as f32 / 1024.0),
            fw_num_pages
        )));

        // TODO add check if firmware is valid and fits into flash
        // Check page id (min limit)
        // Check firmware size (max limit)

        // Create app firmware representation
        let mut app_fw = AppFirmware::from_section(app_section);
        app_fw.append_firmware(fw_data)?;

        // Transmit all pages of the firmware to the device
        self._flash_app_pages(&app_fw)?;

        self.progress(ProgressUpdate::Message("Checking CRC".to_string()));
        self._check_app_crc(&app_fw)?;

        self.progress(ProgressUpdate::Message("Flashing App CRC".to_string()));
        self._flash_app_crc(app_fw.get_crc())?;

        self.progress(ProgressUpdate::Message("Starting App".to_string()));
        self.entries
            .get_entry_mut(RequestType::StartApp)
            .exec(&mut self.interface, 0)?;

        self.progress(ProgressUpdate::Message(
            "App successfully flashed & started!".to_string(),
        ));

        Ok(())
    }

    // Getters ------------------------------------------------------------------------------------

    /// Get entry value of request type
    ///
    /// This function returns the value of the entry of the given request type. If no entry is found,
    /// the function panics.
    pub fn get_entry_value(&self, request_type: RequestType) -> u32 {
        self.entries
            .get_entry(request_type)
            .get_value()
            .unwrap()
            .to_word()
    }

    /// Read entry value of request type
    pub fn read_entry_value(&mut self, request_type: RequestType) -> Result<&MsgData, Error> {
        self.entries
            .get_entry_mut(request_type)
            .read_value(&mut self.interface)
    }

    pub fn is_app_crc_valid(&mut self, app: &AppFirmware) -> Result<bool, Error> {
        let app_crc = app.get_crc();
        let dev_crc = self
            .read_entry_value(RequestType::AppInfoCRCCalc)?
            .to_word();
        Ok(app_crc == dev_crc)
    }

    pub fn get_bootloader_version(&self) -> String {
        let version = self
            .entries
            .get_entry(RequestType::DevInfoBootloaderVersion)
            .get_value()
            .unwrap();

        let major = version.get_byte(0);
        let minor = version.get_byte(1);
        let patch = version.get_byte(2);

        format!("{}.{}.{}", major, minor, patch)
    }

    pub fn get_device_info_vid(&self) -> u32 {
        self.get_entry_value(RequestType::DevInfoVID)
    }

    pub fn get_device_info_pid(&self) -> u32 {
        self.get_entry_value(RequestType::DevInfoPID)
    }

    pub fn get_device_info_prd(&self) -> u32 {
        self.get_entry_value(RequestType::DevInfoPRD)
    }

    pub fn get_device_info_uid(&self) -> u128 {
        self.get_entry_value(RequestType::DevInfoUID1) as u128
            | (self.get_entry_value(RequestType::DevInfoUID2) as u128) << 32
            | (self.get_entry_value(RequestType::DevInfoUID3) as u128) << 64
            | (self.get_entry_value(RequestType::DevInfoUID4) as u128) << 96
    }

    // Private Functions --------------------------------------------------------------------------

    /// Report a progress update if a progress callback is configured
    fn progress(&self, update: ProgressUpdate) {
        if let Some(ref progress_fn) = self.progress_fn {
            progress_fn(update);
        }
    }

    fn _add_entry(&mut self, entry_type: EntryType, request_type: RequestType) {
        self.entries.push(Entry::new(entry_type, request_type));
    }

    fn _read_const_data(&mut self) -> Result<(), Error> {
        for entry in self.entries.get_vec().iter_mut() {
            if entry.get_entry_type().is_const() {
                entry.read_value(&mut self.interface)?;
            }
        }

        Ok(())
    }

    fn _flash_app_pages(&mut self, app: &AppFirmware) -> Result<(), Error> {
        let total_pages = app.get_page_lst().len() as u32;

        for (idx, app_page) in app.get_page_lst().iter().enumerate() {
            let current = (idx + 1) as u32;
            self.progress(ProgressUpdate::FlashProgress {
                current,
                total: total_pages,
            });

            let app_section = self.flash_desc.get_section("Application").unwrap();
            let flash_page_id = app_page.get_id() + app_section.get_flash_page_id();

            // Clear page buffer
            self.entries
                .get_entry_mut(RequestType::PageBufferClear)
                .exec(&mut self.interface, 0)?;

            // Write bytes to page buffer
            let fw_page_byte_lst = app_page.get_bytes();

            // One word per message
            for msg_idx in 0..((app.get_flash_page_size() as usize) / 4) {
                let byte_offset = msg_idx * 4;

                // Create data
                let msg_data = MsgData::from_array(&[
                    fw_page_byte_lst[byte_offset],
                    fw_page_byte_lst[byte_offset + 1],
                    fw_page_byte_lst[byte_offset + 2],
                    fw_page_byte_lst[byte_offset + 3],
                ]);

                // Calculate packet id
                let packet_id = (msg_idx % 256) as u8;

                // Write word to page buffer
                self.entries
                    .get_entry_mut(RequestType::PageBufferWriteWord)
                    .write_value(&mut self.interface, packet_id, &msg_data)?;
            }

            // Read CRC value of page buffer from device
            let page_dev_crc = self
                .read_entry_value(RequestType::PageBufferCalcCRC)?
                .to_word();
            let page_calc_crc = app_page.get_crc();

            if page_dev_crc != page_calc_crc {
                return Err(Error::Error(format!(
                    "Page buffer CRC is invalid! Calc: {:#010X} Dev: {:#010X}!",
                    page_calc_crc, page_dev_crc
                )));
            }

            // Erase flash page
            self.entries
                .get_entry_mut(RequestType::FlashWriteErasePage)
                .exec(&mut self.interface, flash_page_id)?;

            // Write page buffer to flash
            self.entries
                .get_entry_mut(RequestType::PageBufferWriteToFlash)
                .exec(&mut self.interface, flash_page_id)?;
        }

        Ok(())
    }

    fn _erase_unused_pages(&mut self, app: &AppFirmware) -> Result<(), Error> {
        let flash_num_pages = self.entries.get_entry_value(RequestType::FlashInfoNumPages);
        let app_start_page_idx = self.entries.get_entry_value(RequestType::AppInfoPageIdx);

        // Loop through all application pages and check if they are used
        for app_page_id in 0..app.get_page_lst().len() as u32 {
            // Calculate absolute flash page id
            let flash_page_id = app_page_id + app_start_page_idx;

            // Check if page is used
            if app.get_page(app_page_id).is_none() {
                self.progress(ProgressUpdate::Message(format!(
                    "Erasing unused [Page: {}/{}]",
                    flash_page_id + 1,
                    flash_num_pages
                )));

                // Erase flash page
                self.entries
                    .get_entry_mut(RequestType::FlashWriteErasePage)
                    .exec(&mut self.interface, flash_page_id)?;
            }
        }

        Ok(())
    }

    fn _check_app_crc(&mut self, app: &AppFirmware) -> Result<(), Error> {
        if !self.is_app_crc_valid(app)? {
            // Sometimes the CRC is not valid, because the new app needs less flash
            // then the new one. If the unused flash is not cleared the CRC value will
            // be wrong.
            self._erase_unused_pages(app)?;

            // Check again if CRC is valid
            if !self.is_app_crc_valid(app)? {
                // CRC still invalid throw error
                return Err(Error::Error(format!(
                    "CRC check failed! App-CRC: {:#010X} Device-App-CRC: {:#010X}",
                    app.get_crc(),
                    self.entries.get_entry_value(RequestType::AppInfoCRCCalc)
                )));
            }
        }

        Ok(())
    }

    fn _flash_app_crc(&mut self, crc_value: u32) -> Result<(), Error> {
        self.entries
            .get_entry_mut(RequestType::FlashWriteAppCRC)
            .exec(&mut self.interface, crc_value)
    }
}

// Tests ------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::francor::franklyboot::com::{
        msg::{Msg, MsgData, ResultType},
        ComSimulator,
    };

    #[test]
    fn device_new_and_init() {
        let com = new_com_sim_with_data();
        let mut device = Device::new(com);
        device.init().unwrap();

        assert_eq!(
            device
                .entries
                .get_entry_value(RequestType::DevInfoBootloaderVersion),
            0x00030201
        );

        assert_eq!(
            device
                .entries
                .get_entry_value(RequestType::DevInfoBootloaderCRC),
            0xDEADBEEF
        );

        assert_eq!(device.entries.get_entry_value(RequestType::DevInfoVID), 1);
        assert_eq!(device.entries.get_entry_value(RequestType::DevInfoPID), 2);
        assert_eq!(device.entries.get_entry_value(RequestType::DevInfoPRD), 3);
        assert_eq!(device.entries.get_entry_value(RequestType::DevInfoUID1), 4);
        assert_eq!(device.entries.get_entry_value(RequestType::DevInfoUID2), 5);
        assert_eq!(device.entries.get_entry_value(RequestType::DevInfoUID3), 6);
        assert_eq!(device.entries.get_entry_value(RequestType::DevInfoUID4), 7);

        assert_eq!(
            device
                .entries
                .get_entry_value(RequestType::FlashInfoStartAddr),
            0x08000000
        );
        assert_eq!(
            device
                .entries
                .get_entry_value(RequestType::FlashInfoPageSize),
            0x0400
        );
        assert_eq!(
            device
                .entries
                .get_entry_value(RequestType::FlashInfoNumPages),
            0x000F
        );
        assert_eq!(
            device.entries.get_entry_value(RequestType::AppInfoPageIdx),
            0x0002
        );
    }

    #[test]
    fn device_new_no_init_get_entry() {
        let device = Device::new(ComSimulator::new());

        assert!(device
            .entries
            .get_entry(RequestType::DevInfoBootloaderVersion)
            .get_value()
            .is_none());

        assert!(device
            .entries
            .get_entry(RequestType::DevInfoBootloaderCRC)
            .get_value()
            .is_none());

        assert!(device
            .entries
            .get_entry(RequestType::DevInfoVID)
            .get_value()
            .is_none());

        assert!(device
            .entries
            .get_entry(RequestType::DevInfoPID)
            .get_value()
            .is_none());

        assert!(device
            .entries
            .get_entry(RequestType::DevInfoPRD)
            .get_value()
            .is_none());

        assert!(device
            .entries
            .get_entry(RequestType::DevInfoUID1)
            .get_value()
            .is_none());

        assert!(device
            .entries
            .get_entry(RequestType::DevInfoUID2)
            .get_value()
            .is_none());

        assert!(device
            .entries
            .get_entry(RequestType::DevInfoUID3)
            .get_value()
            .is_none());

        assert!(device
            .entries
            .get_entry(RequestType::DevInfoUID4)
            .get_value()
            .is_none());

        assert!(device
            .entries
            .get_entry(RequestType::FlashInfoStartAddr)
            .get_value()
            .is_none());

        assert!(device
            .entries
            .get_entry(RequestType::FlashInfoPageSize)
            .get_value()
            .is_none());

        assert!(device
            .entries
            .get_entry(RequestType::FlashInfoNumPages)
            .get_value()
            .is_none());

        assert!(device
            .entries
            .get_entry(RequestType::AppInfoPageIdx)
            .get_value()
            .is_none());
    }

    #[test]
    #[should_panic]
    fn deview_new_not_init_get_value() {
        let device = Device::new(ComSimulator::new());
        device.get_entry_value(RequestType::DevInfoBootloaderVersion);
    }

    // Helpers ------------------------------------------------------------------------------------

    fn new_com_sim_with_data() -> ComSimulator {
        let mut interface = ComSimulator::new();

        interface.add_response(Msg::new(
            RequestType::DevInfoBootloaderVersion,
            ResultType::Ok,
            0,
            &MsgData::from_word(0x00030201),
        ));

        interface.add_response(Msg::new(
            RequestType::DevInfoBootloaderCRC,
            ResultType::Ok,
            0,
            &MsgData::from_word(0xDEADBEEF),
        ));

        interface.add_response(Msg::new(
            RequestType::DevInfoVID,
            ResultType::Ok,
            0,
            &MsgData::from_word(0x00000001),
        ));

        interface.add_response(Msg::new(
            RequestType::DevInfoPID,
            ResultType::Ok,
            0,
            &MsgData::from_word(0x00000002),
        ));

        interface.add_response(Msg::new(
            RequestType::DevInfoPRD,
            ResultType::Ok,
            0,
            &MsgData::from_word(0x00000003),
        ));

        interface.add_response(Msg::new(
            RequestType::DevInfoUID1,
            ResultType::Ok,
            0,
            &MsgData::from_word(0x00000004),
        ));

        interface.add_response(Msg::new(
            RequestType::DevInfoUID2,
            ResultType::Ok,
            0,
            &MsgData::from_word(0x00000005),
        ));

        interface.add_response(Msg::new(
            RequestType::DevInfoUID3,
            ResultType::Ok,
            0,
            &MsgData::from_word(0x00000006),
        ));

        interface.add_response(Msg::new(
            RequestType::DevInfoUID4,
            ResultType::Ok,
            0,
            &MsgData::from_word(0x00000007),
        ));

        interface.add_response(Msg::new(
            RequestType::FlashInfoStartAddr,
            ResultType::Ok,
            0,
            &MsgData::from_word(0x08000000),
        ));

        interface.add_response(Msg::new(
            RequestType::FlashInfoPageSize,
            ResultType::Ok,
            0,
            &MsgData::from_word(0x00000400),
        ));

        interface.add_response(Msg::new(
            RequestType::FlashInfoNumPages,
            ResultType::Ok,
            0,
            &MsgData::from_word(0x0000000F),
        ));

        interface.add_response(Msg::new(
            RequestType::AppInfoPageIdx,
            ResultType::Ok,
            0,
            &MsgData::from_word(0x00000002),
        ));

        interface
    }
}
