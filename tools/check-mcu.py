#!/usr/bin/env python3
"""Compile portable libraries for a bare-metal target; does not link firmware."""
import argparse
import pathlib
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[1]
# Keep this list explicit: desktop adapters and full Vello/Parley/Taffy stacks
# are deliberately outside the current portable-library contract.
PACKAGES = (
    "zui-types", "zui-input", "zui-paint", "zui-theme", "zui-api",
    "zui-platform", "zui-edit", "zui-virtual", "zui-core",
    "zui-widgets", "zui-extras",
)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", default="thumbv7em-none-eabihf",
                        choices=("thumbv6m-none-eabi", "thumbv7em-none-eabi",
                                 "thumbv7em-none-eabihf", "riscv32imac-unknown-none-elf"),
                        help="installed bare-metal Rust target (default: Cortex-M4F/M7F)")
    args = parser.parse_args()
    command = ["cargo", "build", "--lib", "--target", args.target,
               "--no-default-features", "--features", "zui-extras/dashboard", "--locked", "--offline"]
    for package in PACKAGES:
        command.extend(["-p", package])
    # Missing toolchain components or dependencies fail visibly; never fall back
    # to the host, auto-install tools, or silently omit a portable package.
    result = subprocess.run(command, cwd=ROOT, check=False)
    if result.returncode == 0:
        print(f"Portable library build passed: {len(PACKAGES)} packages on {args.target}.")
        print("Firmware linking, allocator, device rendering and memory budgets remain unverified.")
    return result.returncode


if __name__ == "__main__":
    raise SystemExit(main())
