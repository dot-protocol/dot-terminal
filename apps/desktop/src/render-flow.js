// No already-received old-grid parser work may cross a controller's resize request. The server's
// SIGWINCH can cause output before its resize acknowledgement reaches the client.
export async function orderedResize({drain,isCurrent,prepareGrid,send,recover}) {
  await drain();
  if(!isCurrent())return false;
  prepareGrid();
  try { await send(); } catch(error) {
    if(isCurrent() && recover) await recover();
    throw error;
  }
  return isCurrent();
}
