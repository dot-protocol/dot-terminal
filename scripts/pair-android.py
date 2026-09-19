#!/usr/bin/env python3
"""Bootstrap a Keystore-backed Android peer over authorized adb; application traffic uses TLS over IP.

Requires a built/installed debug APK, initialized node directory and explicit
service grants. This developer enrollment helper is not a public pairing UI.
"""
import argparse
import base64
import json
import os
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = 'world.dot.terminal.dev'
p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--serial', required=True)
p.add_argument('--node-dir', type=Path, required=True)
p.add_argument('--host', required=True)
p.add_argument('--port', type=int, default=17843)
p.add_argument('--name', default='android')
p.add_argument('--terminal', action='store_true')
p.add_argument('--clipboard-read', action='store_true')
p.add_argument('--clipboard-write', action='store_true')
a = p.parse_args()
if not 1 <= a.port <= 65535:
    p.error('invalid port')
adb = ['adb', '-s', a.serial]
activity = PACKAGE + '/world.dot.terminal.MainActivity'
subprocess.run(adb + ['shell', 'am', 'force-stop', PACKAGE], check=True)
# Export afresh after startup so a previous build cannot leave a stale certificate.
subprocess.run(adb + ['shell', 'run-as', PACKAGE, 'rm', '-f', 'files/device-cert.der'], check=True)
subprocess.run(adb + ['shell', 'am', 'start', '-S', '-n', activity], check=True, stdout=subprocess.DEVNULL)
end = time.monotonic() + 10
while True:
    cert = subprocess.run(adb + ['exec-out', 'run-as', PACKAGE, 'cat', 'files/device-cert.der'], capture_output=True)
    if cert.returncode == 0 and 128 <= len(cert.stdout) <= 8192 and cert.stdout[:1] == b'\x30':
        break
    if time.monotonic() > end:
        raise SystemExit('App device identity unavailable; unlock phone and open DOT Terminal')
    time.sleep(0.2)
# The exported certificate is public; the private key cannot be exported by this helper.
cert_path = a.node_dir / 'android-certificate.der'
with open(cert_path, 'wb') as f:
    os.fchmod(f.fileno(), 0o600)
    f.write(cert.stdout)
cmd = [str(ROOT / 'target/debug/dot-terminal-node'), '--state-dir', str(a.node_dir),
       'pair', '--cert', str(cert_path), '--name', a.name]
for flag, enabled in [('terminal', a.terminal), ('clipboard-read', a.clipboard_read), ('clipboard-write', a.clipboard_write)]:
    if enabled:
        cmd.append('--' + flag)
subprocess.run(cmd, check=True)
server_cert = (a.node_dir / 'device-cert.der').read_bytes()
profile = json.dumps({'host': a.host, 'port': a.port,
                      'certificate': base64.b64encode(server_cert).decode()}).encode()
subprocess.run(adb + ['shell', 'run-as', PACKAGE, 'tee', 'files/peer.json'], input=profile,
               stdout=subprocess.DEVNULL, check=True)
subprocess.run(adb + ['shell', 'am', 'start', '-S', '-n', activity], check=True, stdout=subprocess.DEVNULL)
print('Mutual device enrollment complete. Application traffic now uses the paired IP endpoint.')
