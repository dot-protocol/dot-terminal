// Layout only: no terminal authority, credentials or transport.
export const shellMarkup = `
<aside><div class="brand"><b class="mark">●</b> DOT <span>TERMINAL</span></div>
<div class="workspace">PERSONAL WORKSPACE <span class="online">●</span></div>
<button id="new" class="primary">＋ New terminal <kbd>⌘ N</kbd></button>
<div class="section">DOT SESSIONS <button id="refresh" aria-label="Refresh sessions">↻</button></div><nav id="sessions"></nav>
<div class="section">CONNECTED APPS <span>LOCAL</span></div><button id="iterm">▣ iTerm sessions</button><nav id="iterm-list"></nav>
<div class="bottom"><div class="identity">◈ <div>This Mac<small>Local session owner</small></div><i></i></div><p>Your shells keep running when this window closes.</p></div></aside>
<main><header><button id="menu" aria-label="Toggle sessions" aria-expanded="false">☰</button><div><span class="eyebrow">ONE SESSION. ANY SCREEN.</span><h1 id="title">Your command center</h1></div><div class="header-actions"><button id="appearance" aria-label="Appearance" title="Appearance">◐</button><button id="activity" aria-label="Session trajectory" title="Session trajectory" aria-expanded="false">⋮</button><button id="system" aria-label="System and session signals" title="System and session signals">◉</button><button id="browser" aria-label="Open in browser" title="Open in browser">↗</button><button id="control">Take control</button><button id="detach">Release</button></div></header>
<div class="infobar"><span id="state" role="status" aria-live="polite">Choose a session or start a new shell</span><span id="input-state" data-state="view-only" aria-live="polite">VIEW ONLY</span><span id="mode">LOCAL · PRIVATE</span></div>
<div id="terminal"><div id="welcome"><div class="orb">●</div><h2>A home for your work.</h2><p>Persistent shells. Connected devices.<br>Pick up exactly where you left off.</p><button id="start">Start a terminal →</button><small>Existing iTerm sessions are available in the sidebar.</small></div></div>
<footer><span id="details">DOT / development preview</span><button id="sync" aria-label="Session synchronization details">Sync · unknown</button></footer></main>`;
