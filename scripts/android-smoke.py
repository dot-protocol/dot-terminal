#!/usr/bin/env python3
"""Build, push, verify, run, and remove a native ARM64 Android core probe.

Requires ANDROID_NDK_HOME and one authorized device (or ANDROID_SERIAL).
No app installation and no access to personal app data.
"""
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
TARGET = "aarch64-linux-android"


def run(*args, **kwargs):
    return subprocess.run(args, check=True, text=True, capture_output=True, **kwargs).stdout.strip()


def main():
    ndk = os.environ.get("ANDROID_NDK_HOME")
    if not ndk:
        raise SystemExit("Set ANDROID_NDK_HOME to an installed Android NDK directory.")
    host = {"Darwin": "darwin-x86_64", "Linux": "linux-x86_64"}.get(platform.system())
    if host is None:
        raise SystemExit("The build runner currently supports macOS and Linux hosts.")
    linker = Path(ndk) / "toolchains/llvm/prebuilt" / host / "bin/aarch64-linux-android28-clang"
    if not linker.is_file():
        raise SystemExit(f"Android linker not found: {linker}")
    serial = os.environ.get("ANDROID_SERIAL")
    if not serial:
        devices = [r.split()[0] for r in run("adb", "devices").splitlines()[1:]
                   if len(r.split()) >= 2 and r.split()[1] == "device"]
        if len(devices) != 1:
            raise SystemExit("Connect exactly one authorized device, or set ANDROID_SERIAL.")
        serial = devices[0]
    adb = ["adb", "-s", serial]
    if run(*adb, "shell", "getprop", "ro.product.cpu.abi") != "arm64-v8a":
        raise SystemExit("This probe build requires an ARM64 Android device.")
    env = os.environ.copy()
    env["CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER"] = str(linker)
    # Make artifact location explicit even when the caller has a shared Cargo target directory.
    target_dir = ROOT / "target/android-probe"
    subprocess.run(["cargo", "build", "--locked", "--release", "--target", TARGET,
                    "--target-dir", str(target_dir), "-p", "dot-terminal-device-probe"],
                   cwd=ROOT, env=env, check=True)
    binary = target_dir / TARGET / "release/dot-terminal-device-probe"
    digest = hashlib.sha256(binary.read_bytes()).hexdigest()
    remote = run(*adb, "shell", "mktemp", "-d", "/data/local/tmp/dot-terminal-probe.XXXXXXXX")
    if not re.fullmatch(r"/data/local/tmp/dot-terminal-probe\.[A-Za-z0-9]+", remote):
        raise SystemExit("Unexpected temporary device path; refusing to proceed.")
    try:
        executable = remote + "/probe"
        run(*adb, "push", str(binary), executable)
        run(*adb, "shell", "chmod", "700", executable)
        actual = run(*adb, "shell", "sha256sum", executable).split()[0]
        if actual != digest:
            raise SystemExit("Transferred artifact digest mismatch.")
        result = json.loads(run(*adb, "shell", executable))
        if result.get("status") != "pass" or result.get("os") != "android" or result.get("arch") != "aarch64":
            raise SystemExit("Unexpected native probe result.")
        print(json.dumps({
            "source_commit": run("git", "rev-parse", "HEAD", cwd=ROOT),
            "source_dirty": bool(run("git", "status", "--porcelain", cwd=ROOT)),
            "binary_sha256": digest,
            "android_release": run(*adb, "shell", "getprop", "ro.build.version.release"),
            "android_api": run(*adb, "shell", "getprop", "ro.build.version.sdk"),
            "device_model": run(*adb, "shell", "getprop", "ro.product.model"),
            "execution_context": "adb-shell; not Android application sandbox",
            "result": result,
        }, indent=2))
    finally:
        run(*adb, "shell", "rm", "-r", remote)


if __name__ == "__main__":
    try:
        main()
    except subprocess.CalledProcessError as error:
        print(error.stderr or str(error), file=sys.stderr)
        raise SystemExit(error.returncode)
