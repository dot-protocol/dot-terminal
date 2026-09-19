#!/usr/bin/env python3
"""Build the Rust JNI library and Android debug APK using pinned project inputs."""
import os
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]
ndk = Path(os.environ['ANDROID_NDK_HOME'])
host = 'darwin-x86_64' if os.uname().sysname == 'Darwin' else 'linux-x86_64'
linker = ndk / 'toolchains/llvm/prebuilt' / host / 'bin/aarch64-linux-android28-clang'
env = dict(os.environ, CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=str(linker),
           CARGO_TARGET_DIR=str(ROOT / 'target/android-build'),
           CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS='-C link-arg=-Wl,-z,max-page-size=16384')
subprocess.run(['cargo', 'build', '--locked', '--release', '-p', 'dot-terminal-android',
                '--target', 'aarch64-linux-android'], cwd=ROOT, env=env, check=True)
out = ROOT / 'target/android-jni/arm64-v8a'
out.mkdir(parents=True, exist_ok=True)
shutil.copy2(ROOT / 'target/android-build/aarch64-linux-android/release/libdot_terminal_android.so', out)
subprocess.run(['./gradlew', '--no-daemon', ':app:assembleDebug', ':app:lintDebug'],
               cwd=ROOT / 'apps/android', check=True)
print(ROOT / 'apps/android/app/build/outputs/apk/debug/app-debug.apk')
