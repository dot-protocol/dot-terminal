#!/usr/bin/env python3
"""Development-only, one-session USB bridge. Never binds a LAN address.

Run with an existing keeper socket and an explicitly selected adb device. The
random capability is delivered directly to the debug app; never printed or saved
on the Mac. Closing the bridge revokes that capability. Not a production network
transport, encrypted channel, or sandbox. Same-user Mac processes are trusted.
"""
import argparse
import hmac
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
import re
import secrets
import socket
import stat
import os
import struct
import subprocess

LIMIT = 256 * 1024
ALLOWED = {'status', 'screen', 'read', 'acquire', 'input', 'resize', 'release'}

def receive(stream, count):
    result = bytearray()
    while len(result) < count:
        chunk = stream.recv(count - len(result))
        if not chunk:
            raise OSError('keeper closed connection')
        result.extend(chunk)
    return bytes(result)


def make_handler(path, token):
    class Handler(BaseHTTPRequestHandler):
        def setup(self):
            self.request.settimeout(3)
            super().setup()

        def log_message(self, *_):
            pass  # Never log capabilities, commands, or terminal content.

        def reply(self, status, value):
            data = json.dumps(value).encode()
            self.send_response(status)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Cache-Control', 'no-store')
            self.send_header('Content-Length', str(len(data)))
            self.end_headers()
            self.wfile.write(data)

        def do_POST(self):
            if self.path != '/rpc' or not hmac.compare_digest(
                self.headers.get('Authorization', ''), 'Bearer ' + token
            ):
                self.reply(403, {'error': 'forbidden'})
                return
            try:
                if self.headers.get('Transfer-Encoding') or len(self.headers.get_all('Content-Length', [])) != 1:
                    raise ValueError('one content length required')
                size = int(self.headers['Content-Length'])
                if not 0 < size <= LIMIT:
                    raise ValueError('frame limit')
                data = self.rfile.read(size)
                if len(data) != size:
                    raise ValueError('truncated input')
                request = json.loads(data)
                if request.get('operation', {}).get('type') not in ALLOWED:
                    raise ValueError('operation outside this capability')
                # Strict fields, version, dimensions and input sizes are validated by Rust keeper.
                with socket.socket(socket.AF_UNIX) as stream:
                    stream.settimeout(3)
                    stream.connect(path)
                    stream.sendall(struct.pack('>I', len(data)) + data)
                    length = struct.unpack('>I', receive(stream, 4))[0]
                    if length > LIMIT:
                        raise ValueError('response limit')
                    response = json.loads(receive(stream, length))
                self.reply(200, response)
            except (ValueError, OSError, AttributeError, TypeError):
                self.reply(400, {'error': 'request failed'})
    return Handler


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--socket', required=True)
    p.add_argument('--serial', required=True, help='authorized adb device')
    a = p.parse_args()
    info = os.lstat(a.socket)
    parent = os.lstat(os.path.dirname(a.socket))
    if (not stat.S_ISSOCK(info.st_mode) or info.st_uid != os.getuid()
            or info.st_mode & 0o077 or not stat.S_ISDIR(parent.st_mode)
            or parent.st_uid != os.getuid() or parent.st_mode & 0o077):
        p.error('requires an owned private keeper socket in an owned private directory')
    if not re.fullmatch(r'[A-Za-z0-9._:-]+', a.serial):
        p.error('invalid device selector')
    token = secrets.token_hex(32)
    server = HTTPServer(('127.0.0.1', 17842), make_handler(a.socket, token))
    adb = ['adb', '-s', a.serial]
    try:
        subprocess.run(adb + ['reverse', 'tcp:17842', 'tcp:17842'], check=True, stdout=subprocess.DEVNULL)
        # The token is visible to same-user process inspection; this is explicitly a dev transport.
        subprocess.run(adb + ['shell', 'am', 'start', '-S', '-n',
            'world.dot.terminal.dev/world.dot.terminal.MainActivity', '--es', 'pairing', token],
            check=True, stdout=subprocess.DEVNULL)
        print('USB bridge ready. On the phone, tap Take control. Ctrl-C revokes the bridge.', flush=True)
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
        subprocess.run(adb + ['reverse', '--remove', 'tcp:17842'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

if __name__ == '__main__':
    main()
