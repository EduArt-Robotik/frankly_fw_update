//! # Frankly Firmware Update CLI
//!
//! Command-line interface for updating firmware on embedded devices using the Frankly Bootloader protocol.
//!
//! ## Overview
//!
//! This CLI tool provides commands for discovering, erasing, flashing, and resetting embedded devices
//! that use the Frankly Bootloader. It supports multiple communication interfaces including Serial,
//! CAN bus, and a simulated device interface for testing.
//!
//! ## Supported Operations
//!
//! - **search**: Discover devices on the specified network/interface
//! - **erase**: Erase the application section of a device's flash memory
//! - **flash**: Flash new firmware (Intel HEX format) to a device
//! - **reset**: Reset a device (restart)
//!
//! ## Communication Interfaces
//!
//! - **Serial**: UART/USB serial communication (e.g., ttyACM0, ttyUSB0)
//! - **CAN**: CAN bus multi-device network (e.g., can0, vcan0)
//! - **SIM**: Simulated device for testing and development
//! - **Ethernet**: (Not yet implemented)
//!
//! ## Progress Reporting
//!
//! This CLI uses the `indicatif` crate to display professional progress bars during long-running
//! operations (erase and flash). Progress information flows from the common library via callbacks,
//! ensuring the core protocol implementation remains UI-agnostic.
//!
//! ## Architecture
//!
//! The CLI is a thin layer over the `frankly-fw-update-common` library, providing:
//! - Command-line argument parsing with `clap`
//! - User-friendly progress visualization with `indicatif`
//! - Interface selection and device connection management
//!
//! All bootloader protocol logic is handled by the common library, making this CLI a presentation
//! layer focused on user experience.

use clap::{Arg, ArgAction, Command};
use frankly_fw_update_common::francor::franklyboot::{
    com::{
        can::CANInterface, serial::SerialInterface, sim::SIMInterface, ComConnParams, ComInterface,
        ComMode,
    },
    device::Device,
    firmware::hex_file::HexFile,
    Error, ProgressUpdate,
};
use indicatif::{ProgressBar, ProgressStyle};
use std::sync::{Arc, Mutex};

// ================================================================================================
// Constants
// ================================================================================================

/// Default list of simulated device node IDs for testing
const SIM_NODE_LST: [u8; 4] = [1, 3, 31, 8];

// ================================================================================================
// Type Definitions
// ================================================================================================

/// Communication interface type enumeration.
///
/// Represents the available communication protocols for connecting to embedded devices.
/// Each variant corresponds to a different implementation of the `ComInterface` trait:
///
/// - **Sim**: Simulated devices for testing without hardware (no physical interface required)
/// - **Serial**: UART/USB serial connections (single device per port)
/// - **CAN**: CAN bus networks (supports multiple devices on single bus)
/// - **Ethernet**: Future support for Ethernet-based connections (not yet implemented)
pub enum InterfaceType {
    /// Simulated device interface for testing (no hardware required)
    Sim,
    /// Serial/UART interface (e.g., USB-to-serial adapters)
    Serial,
    /// CAN bus interface (supports multi-device networks)
    CAN,
    /// Ethernet interface (placeholder, not yet implemented)
    Ethernet,
}

impl InterfaceType {
    /// Parses an interface type from a command-line string argument.
    ///
    /// Converts user-provided string values to the corresponding `InterfaceType` variant.
    ///
    /// # Arguments
    ///
    /// * `s` - String slice containing the interface type name
    ///
    /// # Returns
    ///
    /// * `Ok(InterfaceType)` - Successfully parsed interface type
    /// * `Err(Error)` - Unknown interface type string
    ///
    /// # Supported Values
    ///
    /// - `"sim"` → `InterfaceType::Sim`
    /// - `"serial"` → `InterfaceType::Serial`
    /// - `"can"` → `InterfaceType::CAN`
    /// - `"ethernet"` → `InterfaceType::Ethernet`
    fn from_str(s: &str) -> Result<Self, Error> {
        match s {
            "sim" => Ok(InterfaceType::Sim),
            "serial" => Ok(InterfaceType::Serial),
            "can" => Ok(InterfaceType::CAN),
            "ethernet" => Ok(InterfaceType::Ethernet),
            _ => Err(Error::Error(format!("Unknown interface type {}", s))),
        }
    }
}

// ================================================================================================
// Main Entry Point
// ================================================================================================

/// Application entry point for the Frankly Firmware Update CLI.
///
/// This function:
/// 1. Initializes simulated devices (for SIM interface support)
/// 2. Parses command-line arguments using clap
/// 3. Dispatches to the appropriate operation handler (search/erase/flash/reset)
/// 4. Routes to the correct interface implementation based on user's `--type` argument
///
/// # Command Structure
///
/// ```text
/// frankly-fw-update <COMMAND> --type <TYPE> --interface <NAME> [--node <ID>] [--hex-file <PATH>]
/// ```
///
/// # Available Commands
///
/// - **search**: Scan network/interface for connected devices
/// - **erase**: Erase application flash section on target device
/// - **flash**: Flash Intel HEX firmware to target device (requires `--hex-file`)
/// - **reset**: Reset target device
/// - **wakeup** : Send wakeup signal to devices (for CAN networks only)
///
/// # Common Arguments
///
/// - `--type`: Interface type (`sim`, `serial`, `can`, `ethernet`)
/// - `--interface`: Interface name (e.g., `can0`, `ttyACM0`, `sim`)
/// - `--node`: Node ID for multi-device networks (required for CAN operations)
/// - `--hex-file`: Path to Intel HEX firmware file (required for flash command)
///
/// # Examples
///
/// Search for devices on CAN interface:
/// ```bash
/// frankly-fw-update search --type can --interface can0
/// ```
///
/// Flash firmware to node 1 on CAN network:
/// ```bash
/// frankly-fw-update flash --type can --interface can0 --node 1 --hex-file firmware.hex
/// ```
///
/// Erase device connected via serial:
/// ```bash
/// frankly-fw-update erase --type serial --interface ttyACM0
/// ```
fn main() {
    create_sim_devices();

    let type_arg = Arg::new("type")
        .long("type")
        .help("Interface type \"sim\", \"serial\", \"can\"")
        .required(true)
        .action(ArgAction::Set)
        .num_args(1);

    let interface_arg = Arg::new("interface")
        .long("interface")
        .help("Interface name \"can0\", \"ttyACM0\", \"sim\"")
        .required(true)
        .action(ArgAction::Set)
        .num_args(1);

    let node_arg = Arg::new("node")
        .long("node")
        .help("Node ID: 1, 2, ..")
        .value_parser(clap::value_parser!(u8).range(0..))
        .required(false)
        .action(ArgAction::Set)
        .num_args(1);

    let matches = Command::new("frankly-fw-update")
        .version("0.2.0")
        .author("Martin Bauernschmitt - FRANCOR e.V.")
        .arg_required_else_help(true)
        .subcommand_required(true)
        .subcommand(
            Command::new("search")
                .long_flag("search")
                .about("Search for connected devices on specified network")
                .arg(type_arg.clone())
                .arg(interface_arg.clone()),
        )
        .subcommand(
            Command::new("erase")
                .long_flag("erase")
                .about("Erases the application from the device")
                .arg(type_arg.clone())
                .arg(interface_arg.clone())
                .arg(node_arg.clone()),
        )
        .subcommand(
            Command::new("flash")
                .long_flag("flash")
                .about("Flashes the application to the device")
                .arg(type_arg.clone())
                .arg(interface_arg.clone())
                .arg(node_arg.clone())
                .arg(
                    Arg::new("hex-file")
                        .long("hex-file")
                        .help("Path to hex file")
                        .required(true)
                        .action(ArgAction::Set)
                        .num_args(1),
                ),
        )
        .subcommand(
            Command::new("reset")
                .long_flag("reset")
                .about("Reset device")
                .arg(type_arg.clone())
                .arg(interface_arg.clone())
                .arg(node_arg.clone()),
        )
        .subcommand(
            Command::new("wakeup")
                .long_flag("wakeup")
                .about("[Application Message] Sends wakeup to trigger bootloader mode")
                .arg(type_arg.clone())
                .arg(interface_arg.clone()),
        )
        .get_matches();

    println!("Frankly Firmware Update CLI (c) 2023 Martin Bauernschmitt - FRANCOR e.V.");

    match matches.subcommand() {
        Some(("search", search_matches)) => {
            let interface_type_str = search_matches.get_one::<String>("type").unwrap();
            let interface_type = InterfaceType::from_str(interface_type_str).unwrap();
            let interface_name = search_matches.get_one::<String>("interface").unwrap();

            match interface_type {
                InterfaceType::Serial => search_for_devices::<SerialInterface>(
                    &ComConnParams::for_serial_conn(interface_name, 115200),
                ),
                InterfaceType::CAN => {
                    search_for_devices::<CANInterface>(&ComConnParams::for_can_conn(interface_name))
                }
                InterfaceType::Ethernet => {
                    println!("Ethernet not supported yet");
                }
                InterfaceType::Sim => {
                    search_for_devices::<SIMInterface>(&ComConnParams::for_sim_device())
                }
            }
        }
        Some(("erase", erase_matches)) => {
            let interface_type_str = erase_matches.get_one::<String>("type").unwrap();
            let interface_type = InterfaceType::from_str(interface_type_str).unwrap();
            let interface_name = erase_matches.get_one::<String>("interface").unwrap();
            let node_id = erase_matches.get_one::<u8>("node").copied();

            match interface_type {
                InterfaceType::Serial => erase_device::<SerialInterface>(
                    &ComConnParams::for_serial_conn(interface_name, 115200),
                    node_id,
                ),
                InterfaceType::CAN => erase_device::<CANInterface>(
                    &ComConnParams::for_can_conn(interface_name),
                    node_id,
                ),
                InterfaceType::Ethernet => println!("Ethernet not supported yet"),
                InterfaceType::Sim => {
                    erase_device::<SIMInterface>(&ComConnParams::for_sim_device(), node_id)
                }
            }
        }
        Some(("flash", flash_matches)) => {
            let interface_type_str = flash_matches.get_one::<String>("type").unwrap();
            let interface_type = InterfaceType::from_str(interface_type_str).unwrap();
            let interface_name = flash_matches.get_one::<String>("interface").unwrap();
            let node_id = flash_matches.get_one::<u8>("node").copied();
            let hex_file_path = flash_matches.get_one::<String>("hex-file").unwrap();

            match interface_type {
                InterfaceType::Serial => flash_device::<SerialInterface>(
                    &ComConnParams::for_serial_conn(interface_name, 115200),
                    node_id,
                    hex_file_path,
                ),
                InterfaceType::CAN => flash_device::<CANInterface>(
                    &ComConnParams::for_can_conn(interface_name),
                    node_id,
                    hex_file_path,
                ),
                InterfaceType::Ethernet => println!("Ethernet not supported yet"),
                InterfaceType::Sim => flash_device::<SIMInterface>(
                    &ComConnParams::for_sim_device(),
                    node_id,
                    hex_file_path,
                ),
            }
        }
        Some(("reset", reset_matches)) => {
            let interface_type_str = reset_matches.get_one::<String>("type").unwrap();
            let interface_type = InterfaceType::from_str(interface_type_str).unwrap();
            let interface_name = reset_matches.get_one::<String>("interface").unwrap();
            let node_id = reset_matches.get_one::<u8>("node").copied();

            match interface_type {
                InterfaceType::Serial => reset_device::<SerialInterface>(
                    &ComConnParams::for_serial_conn(interface_name, 115200),
                    node_id,
                ),
                InterfaceType::CAN => reset_device::<CANInterface>(
                    &ComConnParams::for_can_conn(interface_name),
                    node_id,
                ),
                InterfaceType::Ethernet => println!("Ethernet not supported yet"),
                InterfaceType::Sim => {
                    reset_device::<SIMInterface>(&ComConnParams::for_sim_device(), node_id)
                }
            }
        }
        Some(("wakeup", wakeup_matches)) => {
            let interface_type_str = wakeup_matches.get_one::<String>("type").unwrap();
            let interface_type = InterfaceType::from_str(interface_type_str).unwrap();
            let interface_name = wakeup_matches.get_one::<String>("interface").unwrap();

            match interface_type {
                InterfaceType::Serial => println!("Serial not supported yet"),
                InterfaceType::CAN => wakeup_devices(&ComConnParams::for_can_conn(interface_name)),
                InterfaceType::Ethernet => println!("Ethernet not supported yet"),
                InterfaceType::Sim => println!("Sim not supported yet"),
            }
        }
        _ => {
            println!("Unknown command");
        }
    }
}

// ================================================================================================
// Device Connection and Operation Functions
// ================================================================================================

/// Connects to a bootloader device via the specified interface.
///
/// This is the primary connection function used by all operations. It handles:
/// - Creating and opening the communication interface
/// - Setting the communication mode (broadcast vs. specific node)
/// - Initializing the device (reading device info, flash layout, etc.)
/// - Attaching an optional progress callback for operation feedback
///
/// # Type Parameters
///
/// * `I` - Communication interface type implementing `ComInterface`
///
/// # Arguments
///
/// * `conn_params` - Connection parameters (interface name, baud rate, etc.)
/// * `node_id` - Optional node ID for multi-device networks (required for CAN)
/// * `progress_fn` - Optional callback for receiving progress updates during operations
///
/// # Returns
///
/// * `Ok(Device<I>)` - Successfully connected and initialized device
/// * `Err(Error)` - Connection failed, interface open failed, or device initialization failed
///
/// # Progress Callback
///
/// The progress callback receives `ProgressUpdate` enum values:
/// - `Message(String)`: General status messages
/// - `EraseProgress { current, total }`: Page-by-page erase progress
/// - `FlashProgress { current, total }`: Page-by-page flash progress
///
/// # Examples
///
/// Connect to a serial device without progress reporting:
/// ```ignore
/// let device = connect_device::<SerialInterface>(
///     &ComConnParams::for_serial_conn("ttyACM0", 115200),
///     None,
///     None,
/// )?;
/// ```
///
/// Connect to a CAN device with progress bar:
/// ```ignore
/// let progress_fn = Some(Box::new(|update| {
///     println!("{:?}", update);
/// }) as Box<dyn Fn(ProgressUpdate) + Send>);
///
/// let device = connect_device::<CANInterface>(
///     &ComConnParams::for_can_conn("can0"),
///     Some(1),
///     progress_fn,
/// )?;
/// ```
pub fn connect_device<I>(
    conn_params: &ComConnParams,
    node_id: Option<u8>,
    progress_fn: Option<Box<dyn Fn(ProgressUpdate) + Send>>,
) -> Result<Device<I>, Error>
where
    I: ComInterface,
{
    let mut interface = I::create()?;
    interface.open(conn_params)?;
    if node_id.is_some() {
        interface.set_mode(ComMode::Specific(node_id.unwrap()))?;
    }

    let mut device = Device::new_with_progress(interface, progress_fn);
    device.init()?;

    Ok(device)
}

/// Searches for and discovers devices on the specified interface.
///
/// Performs device discovery with behavior that varies by interface type:
/// - **Network Interfaces (CAN, SIM)**: Scans the network using `scan_network()` to discover
///   all responding nodes, then connects to each individually to retrieve device information
/// - **Point-to-Point (Serial)**: Directly connects to the single device on the port
///
/// Discovered devices are printed to stdout with their node ID (for networks) and full device info.
///
/// # Type Parameters
///
/// * `I` - Communication interface type implementing `ComInterface`
///
/// # Arguments
///
/// * `conn_params` - Connection parameters for the interface
///
/// # Output Format
///
/// Network interfaces:
/// ```text
/// Device found[  1]: Node   1 - VID: 0x46524352, PID: 0x00000001, PRD: 0x00000001, UID: 0x...
/// Device found[  3]: Node   3 - VID: 0x46524352, PID: 0x00000002, PRD: 0x00000001, UID: 0x...
/// ```
///
/// Point-to-point interfaces:
/// ```text
/// Device found: VID: 0x46524352, PID: 0x00000001, PRD: 0x00000001, UID: 0x...
/// ```
///
/// # Panics
///
/// This function uses `.unwrap()` on all results, so it will panic if:
/// - Interface creation or opening fails
/// - Network scan fails
/// - Device connection or initialization fails
///
/// # Examples
///
/// Search CAN network:
/// ```ignore
/// search_for_devices::<CANInterface>(&ComConnParams::for_can_conn("can0"));
/// ```
///
/// Search serial interface:
/// ```ignore
/// search_for_devices::<SerialInterface>(&ComConnParams::for_serial_conn("ttyACM0", 115200));
/// ```
pub fn search_for_devices<I>(conn_params: &ComConnParams)
where
    I: ComInterface,
{
    if I::is_network() {
        let node_lst = {
            let mut interface = I::create().unwrap();
            interface.open(conn_params).unwrap();
            interface.scan_network().unwrap()
        };

        for node in node_lst {
            let device = connect_device::<I>(conn_params, Some(node), None).unwrap();
            println!("Device found[{:3}]: {}", node, device);
        }
    } else {
        let device = connect_device::<I>(conn_params, None, None).unwrap();
        println!("Device found: {}", device);
    }
}

/// Resets (restarts) a connected device.
///
/// Sends a reset command to the target device, causing it to restart. After reset, the device
/// will either boot into the application firmware (if valid and CRC matches) or remain in the
/// bootloader if no valid application is present.
///
/// # Type Parameters
///
/// * `I` - Communication interface type implementing `ComInterface`
///
/// # Arguments
///
/// * `conn_params` - Connection parameters for the interface
/// * `node_id` - Node ID for network interfaces (required for CAN, ignored for serial)
///
/// # Behavior
///
/// 1. Validates that node_id is provided for network interfaces
/// 2. Connects to the device
/// 3. Displays device information
/// 4. Sends reset command
/// 5. Prints status messages via progress callback
///
/// # Panics
///
/// This function uses `.unwrap()` and will panic if:
/// - Device connection fails
/// - Reset command fails
///
/// # Examples
///
/// Reset device on CAN network:
/// ```ignore
/// reset_device::<CANInterface>(&ComConnParams::for_can_conn("can0"), Some(1));
/// ```
///
/// Reset device on serial interface:
/// ```ignore
/// reset_device::<SerialInterface>(&ComConnParams::for_serial_conn("ttyACM0", 115200), None);
/// ```
pub fn reset_device<I>(conn_params: &ComConnParams, node_id: Option<u8>)
where
    I: ComInterface,
{
    if I::is_network() && node_id.is_none() {
        println!(
            "Node ID required for multi device network interface! Specify with --node <node-id>"
        );
        return;
    }

    let progress_fn = Some(Box::new(|update: ProgressUpdate| {
        if let ProgressUpdate::Message(msg) = update {
            println!("{}", msg)
        }
    }) as Box<dyn Fn(ProgressUpdate) + Send>);

    let mut device = connect_device::<I>(conn_params, node_id, progress_fn).unwrap();
    println!("Device: {}", device);
    device.reset().unwrap();
}

/// Erases the application section of a device's flash memory.
///
/// Erases all pages in the application flash section, preparing the device for new firmware.
/// The bootloader section is write-protected and cannot be erased via this command, ensuring
/// the device can always recover.
///
/// A professional progress bar is displayed during the erase operation, showing:
/// - Elapsed time
/// - Visual progress bar
/// - Current page / total pages
/// - Status message
///
/// # Type Parameters
///
/// * `I` - Communication interface type implementing `ComInterface`
///
/// # Arguments
///
/// * `conn_params` - Connection parameters for the interface
/// * `node_id` - Node ID for network interfaces (required for CAN, ignored for serial)
///
/// # Behavior
///
/// 1. Validates that node_id is provided for network interfaces
/// 2. Creates a progress callback that manages an `indicatif` progress bar
/// 3. Connects to the device with progress reporting enabled
/// 4. Displays device information
/// 5. Executes erase operation with live progress updates
/// 6. Shows completion message when done
///
/// # Progress Bar
///
/// The progress bar is lazily initialized on the first `EraseProgress` update and displays:
/// ```text
/// [00:00:05] ==================>--------------- 18/45 Erasing pages
/// ```
///
/// # Panics
///
/// This function uses `.unwrap()` and will panic if:
/// - Device connection fails
/// - Erase operation fails
/// - Progress bar creation fails
///
/// # Examples
///
/// Erase device on CAN network:
/// ```ignore
/// erase_device::<CANInterface>(&ComConnParams::for_can_conn("can0"), Some(1));
/// ```
///
/// Erase device on serial interface:
/// ```ignore
/// erase_device::<SerialInterface>(&ComConnParams::for_serial_conn("ttyACM0", 115200), None);
/// ```
pub fn erase_device<I>(conn_params: &ComConnParams, node_id: Option<u8>)
where
    I: ComInterface,
{
    if I::is_network() && node_id.is_none() {
        println!(
            "Node ID required for multi device network interface! Specify with --node <node-id>"
        );
        return;
    }

    // Arc<Mutex<>> wrapper allows the progress bar to be shared between the callback closure
    // and the outer scope, enabling lazy initialization on first progress update
    let pb = Arc::new(Mutex::new(Option::<ProgressBar>::None));
    let pb_clone = pb.clone();

    let progress_fn = Some(Box::new(move |update: ProgressUpdate| match update {
        ProgressUpdate::EraseProgress { current, total } => {
            let mut pb_lock = pb_clone.lock().unwrap();
            if pb_lock.is_none() {
                // Lazily create progress bar on first update
                let bar = ProgressBar::new(total as u64);
                bar.set_style(
                    ProgressStyle::default_bar()
                        .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
                        .unwrap()
                        .progress_chars("=>-"),
                );
                bar.set_message("Erasing pages");
                *pb_lock = Some(bar);
            }
            if let Some(ref bar) = *pb_lock {
                bar.set_position(current as u64);
                if current == total {
                    bar.finish_with_message("Erase complete");
                }
            }
        }
        ProgressUpdate::Message(msg) => println!("{}", msg),
        _ => {}
    }) as Box<dyn Fn(ProgressUpdate) + Send>);

    let mut device = connect_device::<I>(conn_params, node_id, progress_fn).unwrap();
    println!("Device: {}", device);
    device.erase().unwrap();
}

/// Flashes firmware to a device from an Intel HEX file.
///
/// Reads an Intel HEX format firmware file, parses it into flash pages, and programs each
/// page to the device's application flash section. This is a comprehensive operation that includes:
/// - Page buffer operations (clear, write, verify CRC)
/// - Flash erase and write operations
/// - Application CRC verification and storage
/// - Application startup
///
/// A professional progress bar is displayed during the flash operation, showing:
/// - Elapsed time
/// - Visual progress bar
/// - Current page / total pages
/// - Status message
///
/// # Type Parameters
///
/// * `I` - Communication interface type implementing `ComInterface`
///
/// # Arguments
///
/// * `conn_params` - Connection parameters for the interface
/// * `node_id` - Node ID for network interfaces (required for CAN, ignored for serial)
/// * `hex_file_path` - Path to the Intel HEX format firmware file
///
/// # Behavior
///
/// 1. Validates that node_id is provided for network interfaces
/// 2. Creates a progress callback that manages an `indicatif` progress bar
/// 3. Connects to the device with progress reporting enabled
/// 4. Displays device information
/// 5. Parses the HEX file into firmware data
/// 6. Executes flash operation with live progress updates
/// 7. Shows completion message when done
///
/// # Flash Process
///
/// For each firmware page:
/// 1. Clear device page buffer
/// 2. Write page data word-by-word to buffer
/// 3. Verify page CRC
/// 4. Erase target flash page
/// 5. Write buffer to flash
/// 6. Verify application CRC
/// 7. Write application CRC to flash
/// 8. Start application
///
/// # Progress Bar
///
/// The progress bar is lazily initialized on the first `FlashProgress` update and displays:
/// ```text
/// [00:00:12] ==================>--------------- 18/45 Flashing pages
/// ```
///
/// # Panics
///
/// This function uses `.unwrap()` and will panic if:
/// - Device connection fails
/// - HEX file parsing fails (invalid format, file not found, etc.)
/// - Flash operation fails (write error, CRC mismatch, etc.)
/// - Progress bar creation fails
///
/// # Examples
///
/// Flash firmware to CAN device:
/// ```ignore
/// flash_device::<CANInterface>(
///     &ComConnParams::for_can_conn("can0"),
///     Some(1),
///     "firmware.hex"
/// );
/// ```
///
/// Flash firmware to serial device:
/// ```ignore
/// flash_device::<SerialInterface>(
///     &ComConnParams::for_serial_conn("ttyACM0", 115200),
///     None,
///     "/path/to/firmware.hex"
/// );
/// ```
pub fn flash_device<I>(conn_params: &ComConnParams, node_id: Option<u8>, hex_file_path: &str)
where
    I: ComInterface,
{
    if I::is_network() && node_id.is_none() {
        println!(
            "Node ID required for multi device network interface! Specify with --node <node-id>"
        );
        return;
    }

    // Arc<Mutex<>> wrapper allows the progress bar to be shared between the callback closure
    // and the outer scope, enabling lazy initialization on first progress update
    let pb = Arc::new(Mutex::new(Option::<ProgressBar>::None));
    let pb_clone = pb.clone();

    let progress_fn = Some(Box::new(move |update: ProgressUpdate| match update {
        ProgressUpdate::FlashProgress { current, total } => {
            let mut pb_lock = pb_clone.lock().unwrap();
            if pb_lock.is_none() {
                // Lazily create progress bar on first update
                let bar = ProgressBar::new(total as u64);
                bar.set_style(
                    ProgressStyle::default_bar()
                        .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
                        .unwrap()
                        .progress_chars("=>-"),
                );
                bar.set_message("Flashing pages");
                *pb_lock = Some(bar);
            }
            if let Some(ref bar) = *pb_lock {
                bar.set_position(current as u64);
                if current == total {
                    bar.finish_with_message("Flash complete");
                }
            }
        }
        ProgressUpdate::Message(msg) => println!("{}", msg),
        _ => {}
    }) as Box<dyn Fn(ProgressUpdate) + Send>);

    let mut device = connect_device::<I>(conn_params, node_id, progress_fn).unwrap();
    println!("Device: {}", device);

    // Parse Intel HEX file into firmware data structure
    let hex_file = HexFile::from_file(hex_file_path).unwrap();
    device.flash(&hex_file).unwrap();
}

/// Sends a single CAN broadcast "wake-up" message to running applications, in order to trigger a restart bootloader mode.
///
/// This is intentionally minimal: it does **not** run the usual `connect_device` handshake
/// (no device info read-out, no node targeting). It just opens the raw CAN interface and
/// transmits one broadcast frame, then counts the amount of targets that responded to the wake-up request.
///
/// # Arguments
///
/// * `conn_params` - Connection parameters for the CAN interface (interface name, e.g. `can0`)
///
/// # Panics
///
/// This function uses `.unwrap()` and will panic if the CAN interface cannot be opened or the
/// frame cannot be sent.
///
/// # Examples
///
/// ```ignore
/// wakeup_devices(&ComConnParams::for_can_conn("can0"));
/// ```
pub fn wakeup_devices(conn_params: &ComConnParams) {
    let mut interface = CANInterface::create().unwrap();
    interface.open(conn_params).unwrap();
    interface.wakeup().expect("Failed to send wake-up message");
}

// ================================================================================================
// Helper Functions
// ================================================================================================

/// Initializes the simulated device network for testing.
///
/// Configures the SIM interface with a predefined list of node IDs from `SIM_NODE_LST`.
/// This function is called automatically at the start of `main()` to ensure simulated
/// devices are available when using the `--type sim` interface option.
///
/// # Behavior
///
/// Converts `SIM_NODE_LST` constant into a vector and passes it to `SIMInterface::config_nodes()`,
/// which configures the C++ device simulator (via FFI) to respond to the specified node IDs.
///
/// # Panics
///
/// Will panic if SIM interface configuration fails (rare, typically only occurs if
/// C++ simulator initialization fails).
///
/// # Note
///
/// This function is always called even when not using the SIM interface, but has minimal
/// overhead as it only configures in-memory data structures.
fn create_sim_devices() {
    let node_lst = SIM_NODE_LST.to_vec();
    SIMInterface::config_nodes(node_lst).unwrap();
}

// ================================================================================================
// Tests
// ================================================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests that simulated device network initialization succeeds.
    ///
    /// Verifies that `SIMInterface::config_nodes()` can successfully configure
    /// a list of simulated node IDs without errors. This validates the FFI bridge
    /// to the C++ device simulator.
    #[test]
    fn test_sim_net_ping() {
        let node_lst = vec![1, 3, 31, 8];
        SIMInterface::config_nodes(node_lst).unwrap();
    }
}
