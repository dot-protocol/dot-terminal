#!/usr/bin/env python3
"""See and steer a running DOT Terminal view from the machine it runs on, without pixels.

  dot-view.py <state-dir>                 what the view shows now, in a few lines
  dot-view.py <state-dir> controls        every control: role, name, state, box
  dot-view.py <state-dir> json            the raw snapshot
  dot-view.py <state-dir> act <action>    reload | refresh | activity:open | activity:close |
                                          select:<session-prefix> | snapshot

The view writes <state-dir>/view-snapshot.json (no terminal text). Actions are interface
navigation only; the page refuses anything else. Use scripts/capture-app.sh when you need pixels.
"""
import json, os, sys, time

def main():
    if len(sys.argv) < 2: sys.exit(__doc__)
    d, cmd = sys.argv[1], (sys.argv[2] if len(sys.argv) > 2 else 'show')
    if cmd == 'act':
        path = os.path.join(d, 'view-actions.json'); pending = []
        if os.path.exists(path):
            try: pending = json.load(open(path))
            except ValueError: pending = []
        fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
        os.write(fd, json.dumps(pending + sys.argv[3:]).encode()); os.close(fd); print('queued', sys.argv[3:]); return
    s = json.load(open(os.path.join(d, 'view-snapshot.json')))
    if cmd == 'json': print(json.dumps(s, indent=1)); return
    if cmd == 'controls':
        for c in s['controls']:
            flags = ' '.join(k for k in ('disabled', 'expanded', 'selected', 'checked', 'unnamed') if c.get(k))
            print(f"{c['role']:<11} {c.get('id','-'):<16} {c['name'][:44]:<44} {flags:<18} {c['box']}")
        return
    age = time.time() - time.mktime(time.strptime(s['at'][:19], '%Y-%m-%dT%H:%M:%S')) + time.timezone
    w, t, x = s['window'], s.get('terminal'), s['text']
    print(f"view     {s['view']['label']} ({s['view']['kind']}) build {s['build']} · snapshot {age:.0f}s old")
    print(f"window   {w['inner'][0]}x{w['inner'][1]} @{w['dpr']}x at {w['position']} on a {w['screen'][0]}x{w['screen'][1]} screen (origin {w['screenOrigin']}) · {'visible' if w['visible'] else 'HIDDEN'} · {'focused' if w['focused'] else 'not focused'}")
    print('layout   ' + ' '.join(f"{k}={v}" for k, v in s['layout'].items() if v))
    print(f"session  {s.get('session')}")
    if t: print(f"terminal {t['cols']}x{t['rows']} · {t['lines']} lines · viewport top {t['viewportTop']} of {t['scrollbackTop']} · {'at bottom' if t['atBottom'] else 'SCROLLED UP'}{' · HIDDEN (loading history)' if t['hidden'] else ''}")
    print(f"text     title={x['title']!r} status={x['status']!r} input={x['input']!r} sync={x['sync']!r} version={x['version']!r}")
    if x.get('agentNow'): print(f"agent    {x['agentNow']}")
    print('people   ' + (' | '.join(x['presence']) or '-'))
    print('devices  ' + ' | '.join(f"{d_['name']} [{d_['state']}] {d_['sessions']} sessions" for d_ in s['devices']))
    a = s['accessibility']; print(f"a11y     {a['controls']} controls · unnamed: {a['unnamed'] or 'none'} · landmarks: {', '.join(a['landmarks'])}")
    print('timeline ' + ' → '.join(f"{e['name']}{'('+str(e['detail'])+')' if 'detail' in e else ''}@{e['t']}ms" for e in s['timeline'][-10:]))

main()
