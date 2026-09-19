# SPDX-License-Identifier: GPL-2.0-or-later
# Copyright 2026 DOT Terminal contributors
"""User-authorized iTerm projection; never adopts ownership of an existing PTY."""
import asyncio
import json
import sys
import iterm2

async def main(connection):
    while True:
        line = await asyncio.to_thread(sys.stdin.readline)
        if not line:
            return
        try:
            request = json.loads(line)
            app = await iterm2.async_get_app(connection)
            sessions = [s for w in app.windows for t in w.tabs for s in t.sessions]
            if request['action'] == 'list':
                result = {'sessions': [{'id': s.session_id, 'name': s.name,
                          'cols': s.grid_size.width, 'rows': s.grid_size.height} for s in sessions]}
            else:
                session = next(s for s in sessions if s.session_id == request['id'])
                if request['action'] == 'screen':
                    screen = await session.async_get_screen_contents()
                    result = {'lines': [screen.line(i).string for i in range(screen.number_of_lines)],
                              'cols': session.grid_size.width, 'rows': session.grid_size.height,
                              'cursor_col': screen.cursor_coord.x, 'cursor_row': screen.cursor_coord.y}
                elif request['action'] == 'input':
                    await session.async_send_text(request['text'], suppress_broadcast=True)
                    result = {'accepted': True}
                else:
                    raise ValueError('unknown operation')
            print(json.dumps(result), flush=True)
        except Exception:
            # Exception strings can include private terminal content; do not forward them.
            print(json.dumps({'error': 'iTerm operation failed; refresh before retrying input.'}), flush=True)

iterm2.run_until_complete(main)
