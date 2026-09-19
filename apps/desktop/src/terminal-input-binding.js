import {readDrop} from './input-controller.js';

// The only place a view listens for input. xterm owns keyboard, IME composition and the paste
// event (shortcut, Edit menu and context menu all raise the same one) and hands over encoded text
// through onData; a drop is inserted with term.paste(), so it gets the same bracketed-paste
// handling as a real paste. Nothing here presses Enter. The keydown listener only COUNTS held-key
// repeats — it never writes — so a held Space can be diagnosed without recording what was typed.
export function bindTerminalInput({term, surface, controller, canDrop, notify}) {
  const stop = [];
  const on = (target, type, handler, options) => { target.addEventListener(type, handler, options); stop.push(() => target.removeEventListener(type, handler, options)); };
  const data = term.onData(text => controller.submit(text)); stop.push(() => data.dispose());
  on(surface, 'keydown', event => { if (event.repeat) controller.sawRepeat(); }, {capture: true, passive: true});

  const meaningful = event => { const t = Array.from(event.dataTransfer?.types ?? []); return t.includes('text/uri-list') || t.includes('text/plain') || t.includes('Files'); };
  on(surface, 'dragover', event => { if (!meaningful(event)) return; event.preventDefault(); event.dataTransfer.dropEffect = canDrop() ? 'copy' : 'none'; surface.classList.add('drop-target'); });
  on(surface, 'dragleave', () => surface.classList.remove('drop-target'));
  on(surface, 'drop', event => {
    surface.classList.remove('drop-target');
    if (!meaningful(event)) return;
    event.preventDefault(); // never let the web view navigate to a dropped link
    const dropped = readDrop(event.dataTransfer);
    if (dropped.kind === 'files') return notify('Dropping files is not supported for this session yet · nothing was sent');
    if (dropped.kind !== 'text') return;
    if (!canDrop()) return notify('Read-only view · take control before dropping text');
    term.paste(dropped.text); term.focus();
    notify('Dropped text inserted · not submitted');
  });
  // A link dropped anywhere else in the window must not replace the app with that page.
  for (const type of ['dragover', 'drop']) on(window, type, event => { if (!event.defaultPrevented && meaningful(event)) event.preventDefault(); });
  return () => { while (stop.length) stop.pop()(); };
}
