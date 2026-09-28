# Frankly Firmware Update Tools

A comprehensive Rust-based toolset for updating firmware on embedded devices using the Frankly Bootloader protocol. The project provides both a command-line interface (CLI) and an interactive terminal user interface (TUI) for managing firmware updates over multiple communication interfaces.

This repo was forked to add some additional features, which are needed in order to effectively communicate with the EduArt pcb-stack.  

## Features

- **Multiple Communication Interfaces**: Serial (UART/USB), CAN bus, and simulated devices
- **Complete Device Management**: Search, erase, flash, and reset operations
- **Two User Interfaces**:
  - **CLI**: Fast, scriptable command-line interface with progress bars
  - **TUI**: Interactive, menu-driven terminal UI with live progress display
- **Network Device Discovery**: Automatic scanning for devices on CAN networks
- **Intel HEX Format Support**: Standard firmware file format
- **Real-time Progress**: Live progress bars and status updates during operations
- **Robust Error Handling**: Comprehensive CRC validation and error reporting

## Table of Contents

- [EduArt goal definition](#eduart-goal-definition)
- [Installation](#installation)
- [Quick Start](#quick-start)
- [CLI Usage](#cli-usage)
- [TUI Usage](#tui-usage)
- [Communication Interfaces](#communication-interfaces)
- [Building from Source](#building-from-source)
- [Architecture](#architecture)
- [Troubleshooting](#troubleshooting)

## EduArt goal definition

This tool shall be used to communicate with any board of the pcb-stack, in order to update/erase/upload new firmware to any board without a programming adapter needed. Currently tested is only the cli-tool using can interface, which can serve as a basis for a end-user-friendly fw update tool.


### Communication

Every board has a bootloader installed, which responds to the can messages of this toolset. 
Each bootloader/board has an individual node_id, in order to be addressed individually by this toolset.


The Can-Identifiers of all bootloaders are determined as: CAN_BASE_ID + node_id * 2 + 1.

The Can-Identifier of this toolset is: CAN_BROADCAST_ID.

**The Can-Identifier of the boards' firmware does not match the Can-Identifier of the boards' bootloader.** This toolset only communicates with bootloaders, it can flash and start the firmware but does not communicate with it. This is important, so that firmware cannot interfere with the bootloader or this toolset.


For Example: 

CAN_BASE_ID = 0x781

CAN_BROADCAST_ID = 0x780

A rpi5-adapter board runs a bootloader with node_id = 1 -> sends CAN-Messages with id 0x784, listens to CAN-Messages sent from 0x780.

An rpi5 running this toolset -> sends CAN-Messages with id 0x780, listens to CAN-Messages sent from 0x780 - 0x78F.

A Power Management might send messages with id 0x508. These messages are ignored both by the bootlader and this toolset.


### Bootloader implementation

Currently there are bootloader implementations for the following boards:

- sensor ring
- rpi5 adapter
- motor shield

Note that in order to use a bootloader, the firmware for the board must reserve the first 16KB of flash memory for the bootloader and the last byte for the CRC. 
Compatible firmware versions for the bootloaders exist, please use them. 


### Wake up, bootloader!

Every bootloader has a wake up trigger implemented: manually restart the board three times and it will stay in bootloader mode, rather than running the firmware application. 

The compatible firmware versions also react to the wake-up CAN-Message BOOTLOADER_START_CMD. 


### Dependencies

Currently, each board's bootloader is managed in it's own repo. Long-term goal is to merge those repos, in order to only maintain one bootloader which can be used by any hardware as a submodule.

Fixed values like the CAN_BROADCAST_ID or the BOOTLOADER_START_CMD are defined in the firmware_library and can be used by any bootloader/firmware implementation. Do not define these elsewhere, the entire pcb-stack needs to follow the same values in order to communicate properly.


## Installation

### From APT Package (Ubuntu/Debian)

The easiest way to install on Ubuntu or Debian systems is using the pre-built `.deb` packages:

```bash
# Download the .deb packages from the latest release
# https://github.com/franc0r/frankly-fw-update-cli/releases

# Install CLI tool
sudo dpkg -i frankly-fw-update-cli_*_amd64.deb

# Install TUI tool
sudo dpkg -i frankly-fw-update-tui_*_amd64.deb

# If you encounter dependency issues, run:
sudo apt-get install -f
```

**Supported Versions**:
- Ubuntu 22.04 LTS (Jammy)
- Ubuntu 24.04 LTS (Noble)
- Debian-based distributions

After installation, the tools are available system-wide:
```bash
frankly-fw-update-cli --help
frankly-fw-update-tui
```

### From Source

#### Prerequisites

**Linux (Ubuntu/Debian)**:
```bash
sudo apt-get update
sudo apt-get install -y libudev-dev
```

**Rust Toolchain**:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup update stable
```

#### Clone and Build

```bash
# Clone the frankly-bootloader dependency (sibling directory)
cd /path/to/your/projects
git clone https://github.com/franc0r/frankly-bootloader.git
cd frankly-bootloader && git checkout devel

# Clone this repository
cd ..
git clone <repository-url> frankly-fw-update-cli
cd frankly-fw-update-cli

# Build the entire workspace
cargo build --release

# The binaries will be available at:
# - target/release/frankly-fw-update-cli
# - target/release/frankly-fw-update-tui
```

## Quick Start

### Using the TUI (Recommended for Beginners)

The Terminal User Interface provides an interactive, menu-driven experience:

```bash
cargo run --release -p frankly-fw-update-tui
```

Navigate using arrow keys, press Enter to select, and follow the on-screen prompts.

### Using the CLI

Search for devices on a CAN interface:
```bash
cargo run --release -p frankly-fw-update-cli -- search --type can --interface can0
```

Flash firmware to a device:
```bash
cargo run --release -p frankly-fw-update-cli -- flash --type can --interface can0 --node 1 --hex-file firmware.hex
```

## CLI Usage

The CLI follows a standard command structure:

```bash
frankly-fw-update-cli <COMMAND> --type <INTERFACE_TYPE> --interface <INTERFACE_NAME> [OPTIONS]
```

### Commands

#### Search for Devices

Discover available devices on the network:

```bash
# Search on CAN interface
cargo run -p frankly-fw-update-cli -- search --type can --interface can0

# Search on Serial interface
cargo run -p frankly-fw-update-cli -- search --type serial --interface ttyACM0

# Search using simulator
cargo run -p frankly-fw-update-cli -- search --type sim --interface sim
```

**Output Example**:
```
Searching for devices...
Found device: Node 1 - VID: 0x46524352, PID: 0x00000001, PRD: 0x00000100
Found device: Node 2 - VID: 0x46524352, PID: 0x00000002, PRD: 0x00000100
Search complete: 2 device(s) found
```

#### Erase Application

Erase the application section of a device's flash memory:

```bash
cargo run -p frankly-fw-update-cli -- erase --type can --interface can0 --node 1
```

**Features**:
- Only erases the application section (bootloader remains intact)
- Shows real-time progress bar
- Validates operation completion

**Output Example**:
```
Erasing application on device...
[00:00:05] [=======================>] 45/45 Erasing pages
Application erased successfully
```

#### Flash Firmware

Write new firmware to a device:

```bash
cargo run -p frankly-fw-update-cli -- flash --type can --interface can0 --node 1 --hex-file path/to/firmware.hex
```

**Features**:
- Automatic hex file parsing and validation
- Page-by-page flashing with CRC verification
- Real-time progress display
- Automatic application CRC writing
- Device restart after successful flash

**Output Example**:
```
Flashing firmware to device...
[00:00:15] [=================>------] 32/45 Flashing pages
[00:00:21] [========================] 45/45 Flashing pages
Firmware flashed successfully
Starting application...
Device reset complete
```

#### Reset Device

Reset the device and start the application:

```bash
cargo run -p frankly-fw-update-cli -- reset --type can --interface can0 --node 1
```

### CLI Options

| Option | Description | Required | Default |
|--------|-------------|----------|---------|
| `--type <TYPE>` | Interface type: `can`, `serial`, `sim` | Yes | - |
| `--interface <NAME>` | Interface name (e.g., `can0`, `ttyACM0`) | Yes | - |
| `--node <ID>` | Target node ID (required for multi-device interfaces) | Conditional | - |
| `--hex-file <PATH>` | Path to Intel HEX firmware file (flash command only) | For flash | - |

### CLI Examples

**Complete Firmware Update Workflow**:
```bash
# 1. Search for devices
cargo run -p frankly-fw-update-cli -- search --type can --interface can0

# 2. Erase existing application
cargo run -p frankly-fw-update-cli -- erase --type can --interface can0 --node 1

# 3. Flash new firmware
cargo run -p frankly-fw-update-cli -- flash --type can --interface can0 --node 1 --hex-file new_firmware.hex

# 4. Reset device (automatically done after flash, but can be done manually)
cargo run -p frankly-fw-update-cli -- reset --type can --interface can0 --node 1
```

**Serial Interface Example**:
```bash
# Search on serial port
cargo run -p frankly-fw-update-cli -- search --type serial --interface ttyUSB0

# Flash over serial (no node ID needed for point-to-point interfaces)
cargo run -p frankly-fw-update-cli -- flash --type serial --interface ttyUSB0 --hex-file firmware.hex
```

## TUI Usage

The Terminal User Interface provides an interactive experience for all operations.

### Launching the TUI

```bash
cargo run --release -p frankly-fw-update-tui
```

### Navigation

| Key | Action |
|-----|--------|
| `↑` / `↓` | Navigate menu options / Browse history |
| `Enter` | Select / Confirm |
| `Tab` | Open file browser (when entering hex file path) |
| `F5` | Refresh device list |
| `Esc` | Go back / Cancel |
| `q` | Quit application |

### TUI Workflow

#### 1. Main Menu

Select the operation you want to perform:
- **Search for Devices**: Discover devices on a network
- **Erase Application**: Clear application flash section
- **Flash Firmware**: Write new firmware to device
- **Reset Device**: Restart device and start application
- **Quit**: Exit the application

#### 2. Interface Selection

Choose the communication interface type:
- **SIM**: Simulated device (for testing)
- **Serial**: UART/USB serial connection
- **CAN**: CAN bus network

#### 3. Interface Name Entry

Enter the interface name (e.g., `can0`, `ttyACM0`, `sim`).

For **Search** operations, the TUI will now scan for devices automatically.

#### 4. Device List (Search Results)

After interface selection for Search, or for operations that require a target device, the TUI displays discovered devices:

```
╔═══════════════════════════════════════╗
║      Select Device                    ║
╠═══════════════════════════════════════╣
║ > Node 1 - VID: 0x46524352           ║
║   Node 2 - VID: 0x46524352           ║
║   Node 3 - VID: 0x46524352           ║
╚═══════════════════════════════════════╝

Press F5 to refresh device list
Press Esc to go back
```

**Features**:
- Automatic device discovery after interface selection
- Shows node ID, Vendor ID (VID), and Product ID (PID)
- Press `F5` to rescan and update the device list
- Select device with arrow keys and Enter

#### 5. Node ID Entry (for non-Search operations)

After selecting a device from the list, the node ID is automatically used for the operation.

#### 6. Hex File Selection (Flash Operation Only)

When flashing firmware, you have two ways to specify the hex file:

**Option A: Manual Entry**
- Type or paste the full path to the hex file
- Press `↑`/`↓` to browse through previously used paths (history)
- The title shows `[History X/Y]` when browsing history
- Type any character to exit history mode and manually edit

**Option B: File Browser**
- Press `Tab` to open the interactive file browser
- Navigate directories with `↑`/`↓` arrow keys
- Press `Enter` on a directory to open it
- Use `..` to go to parent directory
- Only `.hex` files and directories are shown
- Files are color-coded:
  - **Blue**: Directories
  - **Green**: .hex files
- Press `Enter` on a `.hex` file to select it
- Press `Esc` to return to manual entry

**File Browser Example**:
```
╔═══════════════════════════════════════╗
║      Select Hex File                  ║
╠═══════════════════════════════════════╣
║   Path: /home/user/firmware           ║
╠═══════════════════════════════════════╣
║   ..                                  ║
║   bin/                                ║
║ > releases/                           ║
║   app_v1.2.3.hex                      ║
║   bootloader_v2.0.1.hex               ║
╚═══════════════════════════════════════╝

Tab: Toggle browser | Enter: Select | Esc: Back
```

#### 7. Operation Summary

Review the operation details before execution:

```
╔═══════════════════════════════════════╗
║      Confirm Operation                ║
╠═══════════════════════════════════════╣
║ Device: Node 1 - VID: 0x46524352     ║
║ Interface: can (can0)                 ║
║ Command: Flash Firmware               ║
║ Hex File: firmware/app_v1.2.3.hex    ║
╠═══════════════════════════════════════╣
║ > Proceed                             ║
║   Cancel                              ║
╚═══════════════════════════════════════╝
```

Press Enter on "Proceed" to execute or select "Cancel" to abort.

#### 8. Operation Execution

Watch live progress during the operation:

```
╔═══════════════════════════════════════╗
║      Executing: Flash Firmware        ║
╠═══════════════════════════════════════╣
║ Device: Node 1 - VID: 0x46524352     ║
║ Command: Flash Firmware               ║
║                                       ║
║ Progress: [==============>----------] ║
║           32/45 pages (71%)           ║
║                                       ║
║ Flashing page 32/45                   ║
╚═══════════════════════════════════════╝
```

**Features**:
- Real-time progress bar
- Page-by-page status updates
- Percentage completion
- Non-blocking UI (operation runs in background thread)
- Updates every 100ms for smooth animation

#### 9. Results

View operation results with color-coded status:

```
╔═══════════════════════════════════════╗
║      Operation Results                ║
╠═══════════════════════════════════════╣
║ Device: Node 1 - VID: 0x46524352     ║
║ Command: Flash Firmware               ║
║                                       ║
║ Status: ✓ Success                     ║
║                                       ║
║ Firmware flashed successfully         ║
║ 45 pages written                      ║
║ Application CRC verified              ║
║ Device restarted                      ║
╠═══════════════════════════════════════╣
║ Press any key to continue...          ║
╚═══════════════════════════════════════╝
```

Press any key to return to the main menu.

### TUI Features

#### Firmware Path History

The TUI automatically remembers the last 10 firmware file paths you've used:

1. When entering a hex file path, press `↑` to browse older paths
2. Press `↓` to browse newer paths
3. The input title shows `[History X/Y]` when in history mode
4. Start typing to exit history mode and manually edit
5. History persists during the current TUI session

This feature is especially useful when:
- Updating multiple devices with the same firmware
- Testing different firmware versions on the same device
- Switching between development and release builds

#### Device List Refresh (F5)

The device list can be refreshed at any time by pressing `F5`:

1. Automatically rescans the interface for available devices
2. Updates the list with any new or removed devices
3. Useful for detecting hot-plugged devices
4. Shows a scanning message during the refresh

#### Non-blocking Operations

All long-running operations (erase, flash) run in background threads:

- UI remains responsive during operations
- Progress updates flow through message channels
- Can't accidentally interrupt operations with keystrokes
- Smooth, consistent UI updates every 100ms

## Communication Interfaces

### CAN Bus

**Setup**:
```bash
# Bring up CAN interface with 1 Mbit/s
sudo ip link set can0 type can bitrate 1000000
sudo ip link set can0 up
```

**Usage**:
```bash
--type can --interface can0 --node <ID>
```

**Features**:
- Multi-device network support
- Node ID required for targeting specific devices
- Automatic device discovery via broadcast messages
- 1 Mbit/s typical bitrate

### Serial (UART/USB)

**Setup**:
- No special setup required
- Device appears as `/dev/ttyACM0`, `/dev/ttyUSB0`, etc.
- Ensure user has permissions: `sudo usermod -a -G dialout $USER`

**Usage**:
```bash
--type serial --interface ttyUSB0
```

**Features**:
- Point-to-point connection
- No node ID required
- Automatic baud rate negotiation

### Simulator (Development/Testing)

**Usage**:
```bash
--type sim --interface sim
```

**Features**:
- No physical hardware required
- Uses C++ device simulator from frankly-bootloader repository
- Ideal for development and testing
- Full protocol support

## Building from Source

### Project Structure

This is a Cargo workspace with three crates:

```
frankly-fw-update-cli/
├── cli/                    # Command-line interface
├── tui/                    # Terminal user interface
├── common/                 # Core bootloader protocol library
├── debian/                 # Debian packaging files
├── Cargo.toml              # Workspace configuration
├── build-deb.sh            # Local package build script
└── README.md               # This file
```

### Build Commands

```bash
# Build entire workspace
cargo build --release

# Build specific crate
cargo build --release -p frankly-fw-update-cli
cargo build --release -p frankly-fw-update-tui
cargo build --release -p frankly-fw-update-common

# Run tests
cargo test --workspace

# Check code quality
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

### Building Debian/Ubuntu Packages Locally

You can build `.deb` packages locally for testing or distribution:

#### Prerequisites

```bash
# Install Debian packaging tools
sudo apt-get install build-essential debhelper devscripts

# Install Rust and dependencies (if not already installed)
sudo apt-get install cargo rustc libudev-dev pkg-config
```

#### Build Packages

```bash
# Run the build script
./build-deb.sh
```

The script will:
1. Check for required build dependencies
2. Clone the `frankly-bootloader` dependency if needed
3. Build the Debian packages
4. Place the `.deb` files in the parent directory

#### Install Locally Built Packages

```bash
# Install both packages
sudo dpkg -i ../frankly-fw-update-*.deb

# Or install individually
sudo dpkg -i ../frankly-fw-update-cli_*.deb
sudo dpkg -i ../frankly-fw-update-tui_*.deb

# Fix any dependency issues
sudo apt-get install -f
```

#### Manual Package Build

If you prefer to build manually without the script:

```bash
# Build packages using dpkg-buildpackage
dpkg-buildpackage -us -uc -b

# Packages will be created in parent directory
ls ../*.deb
```

### Release Pipeline

The project includes a GitHub Actions workflow (`.github/workflows/release.yml`) that automatically builds packages for multiple Ubuntu versions when a new tag is pushed:

```bash
# Create and push a release tag
git tag v0.1.0
git push origin v0.1.0
```

This will:
1. Build packages for Ubuntu 22.04 and 24.04
2. Create a GitHub release
3. Upload `.deb` packages as release assets
4. Generate release notes

You can also trigger the release workflow manually from GitHub Actions with a custom version number.

### Dependencies

**Runtime Dependencies**:
- `clap`: CLI argument parsing
- `indicatif`: CLI progress bars
- `ratatui`: TUI rendering
- `crossterm`: Terminal manipulation
- `serialport`: Serial communication
- `socketcan`: CAN bus support (Linux only)
- `crc`: CRC-32 validation
- `ihex`: Intel HEX file parsing

**Build Dependencies**:
- `cc`: C++ compiler integration for device simulator
- Requires `frankly-bootloader` repository as sibling directory

## Architecture

### Design Principles

The project follows a clean architecture with clear separation of concerns:

1. **Common Library** (`common/`): Protocol implementation, completely output-agnostic
2. **CLI** (`cli/`): Command-line interface using the common library
3. **TUI** (`tui/`): Interactive terminal UI using the common library

### Key Components

#### Device Abstraction

The `Device<I: ComInterface>` type is generic over communication interfaces, allowing the same device operations across all transport types.

#### Progress Callbacks

The common library uses callbacks for progress reporting:

```rust
pub enum ProgressUpdate {
    Message(String),
    EraseProgress { current: u32, total: u32 },
    FlashProgress { current: u32, total: u32 },
}
```

Applications register callbacks to receive progress updates without coupling the protocol layer to any specific UI.

#### Flash Memory Model

- **Bootloader Section**: Read-only, lower pages
- **Application Section**: Read-write, remaining pages
- Page-based erasure and writing
- CRC validation at page and application level

#### Protocol Details

- 8-byte message structure
- CRC-32/ISO-HDLC validation
- Request-response pattern with packet IDs
- Support for broadcast and unicast modes

## Troubleshooting

### Permission Denied on Serial Port

**Problem**: `Error: Permission denied (os error 13)` when accessing serial port.

**Solution**:
```bash
# Add user to dialout group
sudo usermod -a -G dialout $USER

# Log out and log back in for changes to take effect
```

### CAN Interface Not Found

**Problem**: `Error: Network interface not found: can0`

**Solution**:
```bash
# Check if interface exists
ip link show can0

# Bring up interface
sudo ip link set can0 type can bitrate 1000000
sudo ip link set can0 up

# Verify status
ip -details link show can0
```

### Build Error: frankly-bootloader Not Found

**Problem**: `fatal error: francor/franklyboot/msg.h: No such file or directory`

**Solution**:
```bash
# Clone dependency as sibling directory
cd ..
git clone https://github.com/franc0r/frankly-bootloader.git
cd frankly-bootloader && git checkout devel
cd ../frankly-fw-update-cli
cargo clean
cargo build
```

### Device Not Responding

**Problem**: `Error: ComNoResponse - Device did not respond within timeout`

**Possible Causes**:
1. Device not in bootloader mode
2. Wrong interface or node ID
3. CAN bitrate mismatch
4. Serial port configuration mismatch
5. Device powered off or disconnected

**Solutions**:
- Verify device is in bootloader mode (check device LEDs or indicators)
- Run `search` command to discover correct node IDs
- Check CAN bitrate matches device configuration (typically 1 Mbit/s)
- Try different serial port or interface name
- Check physical connections and power

### Hex File Parse Error

**Problem**: `Error: Failed to parse hex file`

**Solution**:
- Verify file is valid Intel HEX format
- Check file permissions (must be readable)
- Ensure file path is correct (use absolute path or file browser in TUI)
- Try opening file in hex editor to verify format

### Flash Verification Failed

**Problem**: `Error: CRC mismatch after flash`

**Possible Causes**:
1. Communication errors during transfer
2. Flash memory corruption
3. Interference on communication bus

**Solutions**:
- Retry the flash operation
- Erase application first, then flash
- Check for electromagnetic interference on cables
- Reduce CAN bus bitrate if using CAN
- Verify device flash is not write-protected

## Contributing

Contributions are welcome! Please follow these guidelines:

1. Run `cargo fmt --all` before committing
2. Ensure `cargo clippy --workspace --all-targets -- -D warnings` passes
3. Add tests for new features
4. Update documentation for API changes
5. Follow Rust naming conventions and idioms

## License

[Add your license information here]

## Support

For issues, questions, or contributions:
- GitHub Issues: [Add repository URL]
- Documentation: See `CLAUDE.md` for detailed architecture information
- Email: [Add contact information]
