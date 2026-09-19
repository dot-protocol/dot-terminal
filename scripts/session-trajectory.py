#!/usr/bin/env python3
"""Explicit, local Claude JSONL -> metadata-only activity snapshot. No prompt/tool bodies."""
import argparse
import collections
import datetime
import json
import os
from pathlib import Path


def export(source):
    events, calls, results, seen = [], {}, {}, set()
    rows = invalid = 0
    for line in source:
        rows += 1
        try:
            r = json.loads(line)
        except (ValueError, TypeError):
            invalid += 1
            continue
        if not isinstance(r, dict):
            invalid += 1
            continue
        uid = r.get('uuid')
        if uid and uid in seen:
            continue
        if uid:
            seen.add(uid)
        at = r.get('timestamp')
        if not isinstance(at, str):
            continue
        if r.get('subtype') == 'compact_boundary':
            m = r.get('compactMetadata')
            m = m if isinstance(m, dict) else {}
            events.append(dict(kind='compact', at=at, before=m.get('preTokens'), after=m.get('postTokens'), durationMs=m.get('durationMs')))
        message = r.get('message') or {}
        content = message.get('content', []) if isinstance(message, dict) else []
        if isinstance(content, str):
            content = [{'type': 'text', 'text': content}]
        if not isinstance(content, list):
            continue
        if r.get('type') == 'user' and not r.get('isMeta') and not r.get('isCompactSummary') and any(b.get('type') == 'text' for b in content if isinstance(b, dict)):
            events.append(dict(kind='input', at=at))
        for b in content:
            if not isinstance(b, dict):
                continue
            if b.get('type') == 'tool_use' and b.get('id') not in calls:
                name = b.get('name', '')
                name = name if isinstance(name, str) else ''
                category = ('browser' if 'claude-in-chrome' in name else 'shell' if name == 'Bash' else 'files' if name in ('Read', 'Write', 'Edit') else 'tasks' if name in ('TaskCreate', 'TaskUpdate') else 'agent' if name == 'Agent' else 'oracle' if 'oracle' in name else 'other')
                args = b.get('input')
                actions = args.get('actions', []) if isinstance(args, dict) else []
                event = dict(kind='tool', at=at, category=category, state='unresolved', batchSteps=len(actions) if name.endswith('browser_batch') and isinstance(actions, list) else 0)
                calls[b.get('id')] = event
                events.append(event)
            elif b.get('type') == 'tool_result':
                results[b.get('tool_use_id')] = (at, bool(b.get('is_error')))
    for key, event in calls.items():
        if key in results:
            end, error = results[key]
            event.update(end=end, state='error' if error else 'returned')
    events.sort(key=lambda x: x['at'])
    return dict(schema='dot.trajectory.v1', source='claude-jsonl', capturedAt=datetime.datetime.now(datetime.timezone.utc).isoformat(), coverage='Available file only; earlier history and child-agent logs may be absent. Tool return is not proof of task success.', invalidRecords=invalid, events=events)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    with args.source.open() as source:
        data = export(source)
    # Refuse to overwrite any existing file (especially the source transcript).
    fd = os.open(args.output, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, 'w') as output:
        json.dump(data, output, separators=(',', ':'))
    print(json.dumps(dict(events=len(data['events']), kinds=dict(collections.Counter(e['kind'] for e in data['events'])))))


if __name__ == '__main__':
    main()
