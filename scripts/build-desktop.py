#!/usr/bin/env python3
"""Build the macOS desktop bundle. Installation is an explicit --install step."""
import argparse
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
ROOT=Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser();p.add_argument('--install',action='store_true');p.add_argument('--iterm',action='store_true');args=p.parse_args()
if sys.platform!='darwin':raise SystemExit('Native wrapper currently targets macOS; the Rust loopback app also runs on Linux.')
def run(*cmd):subprocess.run(cmd,cwd=ROOT,check=True)
run('npm','ci','--prefix','apps/desktop');run('npm','run','build','--prefix','apps/desktop')
run('cargo','build','--locked','-p','dot-terminal-desktop','-p','dot-terminal','-p','dot-terminal-resources')
bundle=ROOT/'apps/desktop/build/DOT Terminal.app';mac=bundle/'Contents/MacOS';res=bundle/'Contents/Resources';mac.mkdir(parents=True,exist_ok=True);res.mkdir(parents=True,exist_ok=True)
for binary in ['dot-terminal-desktop','dot-terminal','dot-terminal-resources']:shutil.copy2(ROOT/'target/debug'/binary,mac/binary)
shutil.copytree(ROOT/'apps/desktop/dist',res/'web',dirs_exist_ok=True)
shutil.copy2(ROOT/'apps/desktop/iterm_bridge.py',res/'iterm_bridge.py')
shutil.copytree(ROOT/'licenses',res/'Licenses',dirs_exist_ok=True)
shutil.copy2(ROOT/'NOTICE',res/'NOTICE')
shutil.copy2(ROOT/'crates/resources/LICENSE-MIT',res/'Licenses/Resource-Manager-MIT.txt')
run('swiftc','-O',str(ROOT/'apps/desktop/macos/DotTerminal.swift'),'-o',str(mac/'DOTTerminal'))
with (bundle/'Contents/Info.plist').open('wb') as f:plistlib.dump({'CFBundleExecutable':'DOTTerminal','CFBundleIdentifier':'org.dotprotocol.terminal.dev','CFBundleName':'DOT Terminal','CFBundleDisplayName':'DOT Terminal','CFBundleVersion':'1','CFBundleShortVersionString':'0.1.0','NSHighResolutionCapable':True,'LSMinimumSystemVersion':'13.0','NSAppTransportSecurity':{'NSAllowsLocalNetworking':True}},f)
# Optional local Python bridge; renderer and session runtime never depend on it.
legacy=res/'iterm-venv'
if legacy.exists():shutil.rmtree(legacy)
if args.iterm:
 venv=Path.home()/'Library/Application Support/DOT Terminal/integrations/iterm'
 venv.parent.mkdir(parents=True,exist_ok=True)
 run('python3','-m','venv',str(venv))
 run(str(venv/'bin/python3'),'-m','pip','install','-r',str(ROOT/'apps/desktop/requirements-iterm.txt'))
run('codesign','--force','--deep','--sign','-',str(bundle))
if args.install:
 target=Path.home()/'Applications/DOT Terminal.app';target.parent.mkdir(exist_ok=True)
 if target.exists():shutil.rmtree(target)
 shutil.copytree(bundle,target,symlinks=True)
 print(target)
else:print(bundle)
