#!/usr/bin/env python3
"""Build the macOS desktop bundle. Installation is an explicit --install step."""
import argparse
from pathlib import Path
import os
import plistlib
import shutil
import subprocess
import sys
ROOT=Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser();p.add_argument('--identity',default=os.environ.get('DOT_CODESIGN_IDENTITY','-'),help="codesign identity; '-' is ad hoc. A stable identity keeps firewall rules and privacy grants across rebuilds: an ad hoc signature changes with every build, so tools such as LuLu treat each build as a new program and block its first connection until someone answers.");p.add_argument('--install',action='store_true');p.add_argument('--iterm',action='store_true');p.add_argument('--test-app',action='store_true');p.add_argument('--allow-create',action='store_true',help='Allow keepers in the explicitly selected test state');p.add_argument('--test-instance',default='',help='Optional alphanumeric identity for a concurrent test app');p.add_argument('--view-state-dir',type=Path);p.add_argument('--workspace-config',type=Path,help='Private loopback hub config; app becomes a view without starting another backend');args=p.parse_args()
if args.allow_create and (not args.test_app or not args.view_state_dir):raise SystemExit('Allow-create requires explicit test app state')
if args.test_instance and (not args.test_app or not args.test_instance.isalnum()):raise SystemExit('Test instance requires --test-app and an alphanumeric name')
if args.view_state_dir and not args.test_app:raise SystemExit('Shared view requires --test-app')
if args.test_app and args.install:raise SystemExit('Test app must not replace the installed app')
if sys.platform!='darwin':raise SystemExit('Native wrapper currently targets macOS; the Rust loopback app also runs on Linux.')
lab_id='org.dotprotocol.terminal.uxlab'+('.'+args.test_instance if args.test_instance else '')
lab_name='DOT Terminal Lab'+(' '+args.test_instance if args.test_instance else '')+'.app'
build_root=ROOT/'apps/desktop/build/rust-lab' if args.test_app else ROOT/'target'
def run(*cmd):subprocess.run(cmd,cwd=ROOT,check=True)
run('npm','ci','--prefix','packages/terminal');run('npm','ci','--prefix','apps/desktop');run('npm','run','build','--prefix','apps/desktop')
run('cargo','build','--locked','--target-dir',str(build_root),'-p','dot-terminal-desktop','-p','dot-terminal','-p','dot-terminal-resources')
bundle=ROOT/(('apps/desktop/build/'+lab_name) if args.test_app else 'apps/desktop/build/DOT Terminal.app');mac=bundle/'Contents/MacOS';res=bundle/'Contents/Resources';mac.mkdir(parents=True,exist_ok=True);res.mkdir(parents=True,exist_ok=True)
for binary in ['dot-terminal-desktop','dot-terminal','dot-terminal-resources']:shutil.copy2(build_root/'debug'/binary,mac/binary)
shutil.copytree(ROOT/'apps/desktop/dist',res/'web',dirs_exist_ok=True)
shutil.copy2(ROOT/'apps/desktop/iterm_bridge.py',res/'iterm_bridge.py')
shutil.copytree(ROOT/'licenses',res/'Licenses',dirs_exist_ok=True)
shutil.copy2(ROOT/'NOTICE',res/'NOTICE')
shutil.copy2(ROOT/'crates/resources/LICENSE-MIT',res/'Licenses/Resource-Manager-MIT.txt')
run('swiftc','-O',str(ROOT/'apps/desktop/macos/DotTerminal.swift'),'-o',str(mac/'DOTTerminal'))
# A keeper detaches and becomes its own "responsible process", so macOS asks THIS bundle whether a
# program in a DOT session (for example Claude Code dictation) may use the microphone. Without a
# usage description macOS refuses without ever asking. DOT itself never opens the microphone.
# Reaching the owner's other nodes over a private overlay (100.64.0.0/10) counts as local network
# access on macOS. Without this string the connection is dropped and the device shows as offline.
NETWORK_USE='DOT Terminal connects to your other devices, such as a server or phone on your private network, to show and use their terminal sessions.'
MICROPHONE_USE='Programs you run in a DOT Terminal session, such as voice dictation in a coding agent, may ask to use the microphone. DOT Terminal itself does not record audio.'
with (bundle/'Contents/Info.plist').open('wb') as f:plistlib.dump({**({'DOTWorkspaceConfig':str(args.workspace_config.resolve())} if args.workspace_config else {}),**({'DOTLabStateDirectory':str(args.view_state_dir.resolve()),'DOTLabAllowCreate':args.allow_create} if args.view_state_dir else {}),'CFBundleExecutable':'DOTTerminal','CFBundleIdentifier':(lab_id if args.test_app else 'org.dotprotocol.terminal.dev'),'CFBundleName':'DOT Terminal','CFBundleDisplayName':'DOT Terminal','CFBundleVersion':'1','CFBundleShortVersionString':'0.1.0','NSHighResolutionCapable':True,'NSMicrophoneUsageDescription':MICROPHONE_USE,'NSLocalNetworkUsageDescription':NETWORK_USE,'LSMinimumSystemVersion':'13.0','NSAppTransportSecurity':{'NSAllowsLocalNetworking':True}},f)
# Optional local Python bridge; renderer and session runtime never depend on it.
legacy=res/'iterm-venv'
if legacy.exists():shutil.rmtree(legacy)
if args.iterm:
 venv=Path.home()/'Library/Application Support/DOT Terminal/integrations/iterm'
 venv.parent.mkdir(parents=True,exist_ok=True)
 run('python3','-m','venv',str(venv))
 run(str(venv/'bin/python3'),'-m','pip','install','-r',str(ROOT/'apps/desktop/requirements-iterm.txt'))
run('codesign','--force','--deep','--sign',args.identity,str(bundle))
if args.install:
 target=Path.home()/'Applications/DOT Terminal.app';target.parent.mkdir(exist_ok=True)
 if target.exists():shutil.rmtree(target)
 shutil.copytree(bundle,target,symlinks=True)
 print(target)
else:print(bundle)
