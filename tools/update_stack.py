#!/usr/bin/env python3
"""Update all supported boards on a can bus using frankly-fw-update-cli."""

from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys
import tempfile
import time
import zipfile
from dataclasses import dataclass
from pathlib import Path
from typing import Sequence


METADATA_FILENAME = "metadata.json"
BOARD_TYPE_PATTERN = re.compile(r"[a-z0-9]+(?:_[a-z0-9]+)*")
MINIMUM_FIRMWARE_VERSION = (0, 3, 0)
VERSION_PATTERN = re.compile(r"(?<!\d)(\d+)\.(\d+)\.(\d+)(?!\d)")


@dataclass(frozen=True)
class FirmwareImage:
    board_type: str
    version: tuple[int, int, int]
    archive_name: str


@dataclass(frozen=True)
class FlashTarget:
    node: int
    board_type: str
    current_version: tuple[int, int, int]
    image: FirmwareImage


def load_archive_metadata(archive_path: Path) -> dict[str, tuple[int, int, int]]:
    with zipfile.ZipFile(archive_path) as archive:
        metadata_members = [
            member for member in archive.infolist() if member.filename == METADATA_FILENAME
        ]
        if len(metadata_members) != 1:
            raise ValueError(
                f"ZIP archive must contain exactly one root-level {METADATA_FILENAME}"
            )
        try:
            metadata = json.loads(archive.read(metadata_members[0]))
        except (UnicodeDecodeError, json.JSONDecodeError) as error:
            raise ValueError(f"Invalid JSON in {METADATA_FILENAME}: {error}") from error

    if not isinstance(metadata, dict) or not isinstance(metadata.get("boards"), list):
        raise ValueError(f"{METADATA_FILENAME} must contain a 'boards' array")
    if not metadata["boards"]:
        raise ValueError(f"{METADATA_FILENAME} must define at least one board")

    product_ids: dict[str, tuple[int, int, int]] = {}
    seen_product_ids: set[tuple[int, int, int]] = set()
    for board in metadata["boards"]:
        if not isinstance(board, dict):
            raise ValueError("Each board metadata entry must be an object")
        board_type = board.get("type")
        if not isinstance(board_type, str) or not BOARD_TYPE_PATTERN.fullmatch(board_type):
            raise ValueError(f"Invalid board type in {METADATA_FILENAME}: {board_type!r}")
        raw_product_id = board.get("product_id")
        if (
            not isinstance(raw_product_id, list)
            or len(raw_product_id) != 3
            or any(type(part) is not int for part in raw_product_id)
        ):
            raise ValueError(
                f"Product ID for {board_type} must be an integer array [family, identifier, feature]"
            )
        product_id = tuple(raw_product_id)
        family, identifier, feature = product_id
        if not 0 <= family <= 255 or not 0 <= identifier <= 255 or not 0 <= feature <= 65535:
            raise ValueError(f"Product ID components for {board_type} are out of range")
        if board_type in product_ids:
            raise ValueError(f"Duplicate board type in {METADATA_FILENAME}: {board_type}")
        if product_id in seen_product_ids:
            raise ValueError(f"Duplicate product ID in {METADATA_FILENAME}: {product_id}")
        product_ids[board_type] = product_id
        seen_product_ids.add(product_id)
    return product_ids


def _board_in_filename(filename: str, board_types: Sequence[str]) -> str | None:
    normalized = "_" + "_".join(re.findall(r"[a-z0-9]+", Path(filename).stem.lower())) + "_"
    matches = [board for board in board_types if f"_{board}_" in normalized]
    if len(matches) > 1:
        raise ValueError(f"Multiple board types in firmware filename: {filename}")
    return matches[0] if matches else None


def inspect_firmware_archive(
    archive_path: Path, board_types: Sequence[str]
) -> dict[str, FirmwareImage]:
    images: dict[str, FirmwareImage] = {}
    with zipfile.ZipFile(archive_path) as archive:
        for member in archive.infolist():
            if member.is_dir() or Path(member.filename).suffix.lower() != ".hex":
                continue
            board_type = _board_in_filename(member.filename, board_types)
            if board_type is None:
                continue
            versions = VERSION_PATTERN.findall(Path(member.filename).stem)
            if len(versions) != 1:
                raise ValueError(
                    f"Firmware {member.filename} must contain exactly one version X.Y.Z in its filename"
                )
            if board_type in images:
                raise ValueError(
                    f"Multiple firmware files for {board_type} in the ZIP archive; include exactly one"
                )
            images[board_type] = FirmwareImage(
                board_type,
                tuple(int(part) for part in versions[0]),
                member.filename,
            )
    return images


def _require_unique_nodes(devices: Sequence[dict], label: str) -> dict[int, dict]:
    indexed: dict[int, dict] = {}
    for device in devices:
        node = device.get("node")
        if not isinstance(node, int) or not 0 <= node <= 255:
            raise ValueError(f"Invalid node ID in {label} response: {device!r}")
        if node in indexed:
            raise ValueError(f"Node {node} occurs multiple times in the {label} response")
        indexed[node] = device
    return indexed


def create_flash_plan(
    products: Sequence[dict],
    firmware: Sequence[dict],
    images: dict[str, FirmwareImage],
    product_ids: dict[str, tuple[int, int, int]],
    minimum_version: tuple[int, int, int],
) -> list[FlashTarget]:
    products_by_node = _require_unique_nodes(products, "product")
    firmware_by_node = _require_unique_nodes(firmware, "firmware")
    board_by_product_id = {product_id: board for board, product_id in product_ids.items()}
    plan: list[FlashTarget] = []

    for node, product in sorted(products_by_node.items()):
        raw_product_id = product.get("product_id")
        if not isinstance(raw_product_id, list) or len(raw_product_id) != 3:
            raise ValueError(f"Invalid product ID for Node {node}: {raw_product_id!r}")
        product_id = tuple(raw_product_id)
        board_type = board_by_product_id.get(product_id)
        if board_type is None:
            raise ValueError(
                f"Unknown product ID {'.'.join(map(str, product_id))} at Node {node}; "
                f"configure product IDs in {METADATA_FILENAME}"
            )
        image = images.get(board_type)
        if image is None:
            raise ValueError(f"Firmware for {board_type} is missing from the ZIP archive")
        if image.version <= minimum_version:
            raise ValueError(
                f"ZIP version {'.'.join(map(str, image.version))} for {board_type} must be newer "
                f"than {'.'.join(map(str, minimum_version))}"
            )

        firmware_device = firmware_by_node.get(node)
        raw_version = firmware_device.get("version") if firmware_device else None
        if not isinstance(raw_version, list) or len(raw_version) != 3:
            raise ValueError(f"No valid firmware version received for Node {node}")
        current_version = tuple(raw_version)
        if image.version <= current_version:
            raise ValueError(
                f"ZIP version {'.'.join(map(str, image.version))} for Node {node} is not newer "
                f"than the installed version {'.'.join(map(str, current_version))}"
            )
        plan.append(FlashTarget(node, board_type, current_version, image))

    if not plan:
        raise ValueError("No supported boards found; nothing to update")
    return plan


def _resolve_cli(value: str) -> str:
    candidate = Path(value).expanduser()
    if candidate.is_file():
        return str(candidate.resolve())
    if candidate.parent != Path("."):
        raise FileNotFoundError(f"CLI binary not found: {candidate}")
    found = shutil.which(value)
    if found:
        return found
    raise FileNotFoundError(
        f"'{value}' not found in PATH; specify the CLI binary path with --cli"
    )


def _run_json(cli: str, command: str, interface: str) -> dict:
    result = subprocess.run(
        [cli, command, "--type", "can", "--interface", interface, "--json"],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode:
        detail = result.stderr.strip() or result.stdout.strip()
        raise RuntimeError(f"CLI-Befehl '{command}' fehlgeschlagen: {detail}")
    try:
        payload = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise RuntimeError(
            f"CLI command '{command}' did not return valid JSON: {result.stdout.strip()}"
        ) from error
    if not isinstance(payload, dict):
        raise RuntimeError(f"Unexpected CLI response for '{command}': {payload!r}")
    return payload


def _send_wakeup(cli: str, interface: str) -> None:
    result = subprocess.run(
        [cli, "wakeup", "--type", "can", "--interface", interface],
        check=False,
    )
    if result.returncode:
        raise RuntimeError(f"Wake-up command failed (exit code {result.returncode})")


def _wait_for_bootloader_nodes(
    cli: str,
    interface: str,
    expected_nodes: set[int],
    attempts: int,
    interval: float,
    discovered_nodes: set[int],
) -> list[int]:
    if attempts < 1:
        raise ValueError("Bootloader search attempts must be at least 1")

    for attempt in range(1, attempts + 1):
        nodes = _run_json(cli, "search", interface).get("nodes")
        if not isinstance(nodes, list) or any(type(node) is not int for node in nodes):
            raise RuntimeError("CLI bootloader search returned invalid node IDs")
        discovered_nodes.update(nodes)
        found_nodes = set(nodes)
        if len(nodes) == len(found_nodes) and found_nodes == expected_nodes:
            return nodes
        if attempt < attempts:
            print(
                f"Bootloader nodes not ready (found {sorted(found_nodes)}; "
                f"expected {sorted(expected_nodes)}). Retry {attempt}/{attempts - 1} ..."
            )
            time.sleep(max(0.0, interval))

    raise RuntimeError(
        f"Bootloader nodes do not match the product query after {attempts} searches; "
        f"expected {sorted(expected_nodes)}, found {sorted(found_nodes)}. "
        "Nothing will be flashed."
    )


def _extract_image(cli: str, interface: str, node: int, path: Path) -> None:
    result = subprocess.run(
        [
            cli,
            "flash",
            "--type",
            "can",
            "--interface",
            interface,
            "--node",
            str(node),
            "--hex-file",
            str(path),
        ],
        check=False,
    )
    if result.returncode:
        raise RuntimeError(f"Flashing Node {node} failed (exit code {result.returncode})")


def _erase_application(cli: str, interface: str, node: int) -> None:
    result = subprocess.run(
        [cli, "erase", "--type", "can", "--interface", interface, "--node", str(node)],
        check=False,
    )
    if result.returncode:
        raise RuntimeError(f"Erasing Node {node} failed (exit code {result.returncode})")


def _restart_nodes(cli: str, interface: str, nodes: set[int]) -> None:
    if not nodes:
        print(
            "Update failed before any bootloader nodes were found; no restart target is available.",
            file=sys.stderr,
        )
        return

    print(
        f"Update failed; attempting to restart discovered nodes {sorted(nodes)} ...",
        file=sys.stderr,
    )
    for node in sorted(nodes):
        try:
            result = subprocess.run(
                [cli, "restart", "--type", "can", "--interface", interface, "--node", str(node)],
                check=False,
            )
            if result.returncode:
                print(
                    f"Failed to restart Node {node} (exit code {result.returncode})",
                    file=sys.stderr,
                )
        except OSError as error:
            print(f"Failed to restart Node {node}: {error}", file=sys.stderr)


def _confirm(plan: Sequence[FlashTarget], assume_yes: bool) -> bool:
    print("Planned firmware update:")
    for target in plan:
        print(
            f"  Node {target.node}: {target.board_type} "
            f"{'.'.join(map(str, target.current_version))} -> "
            f"{'.'.join(map(str, target.image.version))}"
        )
    if assume_yes:
        return True
    if not sys.stdin.isatty():
        raise RuntimeError("Non-interactive mode requires --yes")
    return input("Start update? [y/N] ").strip().lower() in ("y", "yes")


def _parse_args() -> argparse.Namespace:
    workspace = Path(__file__).resolve().parents[1]
    default_cli = next(
        (
            str(candidate)
            for candidate in (
                workspace / "target/debug/frankly-fw-update-cli",
                workspace / "target/release/frankly-fw-update-cli",
            )
            if candidate.is_file()
        ),
        "frankly-fw-update-cli",
    )
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path, help="ZIP archive containing versioned Intel HEX files")
    parser.add_argument("--interface", default="CAN2", help="CAN interface (default: CAN2)")
    parser.add_argument("--cli", default=default_cli, help="Pfad zur frankly-fw-update-cli")
    parser.add_argument(
        "--bootloader-wait",
        type=float,
        default=3.0,
        help="Wait time after wake-up before searching in seconds (default: 3)",
    )
    parser.add_argument(
        "--bootloader-search-attempts",
        type=int,
        default=5,
        help="Maximum bootloader node searches (default: 5)",
    )
    parser.add_argument(
        "--bootloader-search-interval",
        type=float,
        default=1.0,
        help="Wait between bootloader node searches in seconds (default: 1)",
    )
    parser.add_argument("--dry-run", action="store_true", help="Validate and display the update plan only")
    parser.add_argument("--yes", action="store_true", help="Skip confirmation for automated workflows")
    args = parser.parse_args()
    if args.bootloader_search_attempts < 1:
        parser.error("--bootloader-search-attempts must be at least 1")
    if args.bootloader_search_interval < 0:
        parser.error("--bootloader-search-interval cannot be negative")

    return args


def main() -> int:
    args = _parse_args()
    try:
        cli = _resolve_cli(args.cli)
        if not args.archive.is_file():
            raise FileNotFoundError(f"ZIP archive not found: {args.archive}")
        product_ids = load_archive_metadata(args.archive)
        images = inspect_firmware_archive(args.archive, tuple(product_ids))

        print(f"Checking product IDs and firmware versions on {args.interface} ...")
        products = _run_json(cli, "product", args.interface).get("devices", [])
        firmware = _run_json(cli, "firmware", args.interface).get("devices", [])
        if not isinstance(products, list) or not isinstance(firmware, list):
            raise RuntimeError("CLI response does not contain a valid devices list")
        plan = create_flash_plan(
            products, firmware, images, product_ids, MINIMUM_FIRMWARE_VERSION
        )
        if args.dry_run:
            _confirm(plan, True)
            print("Dry run complete; no devices were modified.")
            return 0
        if not _confirm(plan, args.yes):
            print("Cancelled; no devices were modified.")
            return 0

        print("Sending wake-up to all boards ...")
        _send_wakeup(cli, args.interface)
        discovered_nodes: set[int] = set()
        update_succeeded = False
        try:
            time.sleep(max(0.0, args.bootloader_wait))
            expected_nodes = {target.node for target in plan}
            _wait_for_bootloader_nodes(
                cli,
                args.interface,
                expected_nodes,
                args.bootloader_search_attempts,
                args.bootloader_search_interval,
                discovered_nodes,
            )

            with zipfile.ZipFile(args.archive) as archive, tempfile.TemporaryDirectory(
                prefix="update-"
            ) as temporary_directory:
                temp_path = Path(temporary_directory)
                extracted: dict[str, Path] = {}
                for image in images.values():
                    destination = temp_path / Path(image.archive_name).name
                    with archive.open(image.archive_name) as source, destination.open("wb") as target:
                        shutil.copyfileobj(source, target)
                    extracted[image.board_type] = destination

                for target in plan:
                    print(f"Erasing Node {target.node} ({target.board_type}) ...")
                    _erase_application(cli, args.interface, target.node)
                    print(f"Flashing Node {target.node} ({target.board_type}) ...")
                    _extract_image(cli, args.interface, target.node, extracted[target.board_type])
            update_succeeded = True
        finally:
            if not update_succeeded:
                _restart_nodes(cli, args.interface, discovered_nodes)

        print(f"Firmware update complete: {len(plan)} board(s) updated.")
        return 0
    except (OSError, RuntimeError, ValueError, zipfile.BadZipFile) as error:
        print(f"Error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())