#!/bin/bash
# Local build script for creating Debian/Ubuntu APT packages

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

print_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

print_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check if we're on a Debian-based system
if ! command -v dpkg-buildpackage &> /dev/null; then
    print_error "dpkg-buildpackage not found. This script requires a Debian/Ubuntu system."
    print_info "Install build dependencies with: sudo apt-get install build-essential debhelper"
    exit 1
fi

# Check for required build dependencies
print_info "Checking build dependencies..."

missing_deps=()

# Check for Rust toolchain
if ! command -v cargo &> /dev/null; then
    print_error "Rust toolchain not found (cargo not in PATH)"
    print_info "Install Rust with:"
    echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    echo "  source \$HOME/.cargo/env"
    exit 1
fi

if ! command -v rustc &> /dev/null; then
    print_error "Rust compiler not found (rustc not in PATH)"
    print_info "Install Rust with:"
    echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    echo "  source \$HOME/.cargo/env"
    exit 1
fi

print_info "Found Rust toolchain: rustc $(rustc --version | cut -d' ' -f2)"

# Check for system dependencies
if ! dpkg -l | grep -q libudev-dev; then
    missing_deps+=("libudev-dev")
fi

if ! dpkg -l | grep -q pkg-config; then
    missing_deps+=("pkg-config")
fi

if ! dpkg -l | grep -q debhelper; then
    missing_deps+=("debhelper")
fi

if ! command -v gbp &> /dev/null; then
    missing_deps+=("git-buildpackage")
fi

if [ ${#missing_deps[@]} -ne 0 ]; then
    print_error "Missing system dependencies: ${missing_deps[*]}"
    print_info "Install them with:"
    echo "  sudo apt-get install ${missing_deps[*]}"
    exit 1
fi

print_info "All build dependencies satisfied"

# Check for frankly-bootloader dependency
if [ ! -d "../frankly-bootloader" ]; then
    print_warn "frankly-bootloader dependency not found in parent directory"
    print_info "Cloning frankly-bootloader..."
    git clone --depth 1 --branch devel https://github.com/franc0r/frankly-bootloader.git ../frankly-bootloader
else
    print_info "Found frankly-bootloader dependency"
fi

# Clean previous builds
print_info "Cleaning previous builds..."
cargo clean || true
rm -rf debian/.debhelper debian/frankly-fw-update-* debian/files debian/*.substvars
rm -f ../*.deb ../*.dsc ../*.tar.xz ../*.buildinfo ../*.changes

# Sync version: Cargo.toml -> debian/changelog
CARGO_VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
DEB_REVISION=1
DEB_VERSION="${CARGO_VERSION}-${DEB_REVISION}"
CHANGELOG_VERSION=$(dpkg-parsechangelog -S Version)

if [ -z "$CARGO_VERSION" ]; then
    print_error "Could not read version from Cargo.toml"
    exit 1
fi

if [ "$CHANGELOG_VERSION" != "$DEB_VERSION" ]; then
    print_info "Updating debian/changelog: $CHANGELOG_VERSION -> $DEB_VERSION"
    export DEBFULLNAME="EduArt Robotik GmbH"
    export DEBEMAIL="lea.kempf@eduart-robotik.com"
    gbp dch --release --new-version="$DEB_VERSION" \
        --distribution=unstable --ignore-branch --auto
else
    print_info "debian/changelog already at $DEB_VERSION"
fi

# Build the package
print_info "Building Debian packages..."
print_info "This may take several minutes..."

if dpkg-buildpackage -us -uc -b; then
    print_info "Build completed successfully!"
    echo ""
    print_info "Generated packages:"
    ls -lh ../*.deb 2>/dev/null || print_warn "No .deb files found"
    echo ""
    print_info "To install the packages, run:"
    echo "  sudo dpkg -i ../frankly-fw-update-cli_*.deb"
    echo "  sudo dpkg -i ../frankly-fw-update-tui_*.deb"
    echo ""
    print_info "Or install both at once:"
    echo "  sudo dpkg -i ../frankly-fw-update-*.deb"
    echo ""
    print_info "If you encounter dependency issues, run:"
    echo "  sudo apt-get install -f"
else
    print_error "Build failed!"
    exit 1
fi
