// No old-grid parser work may cross a controller's resize request. The server's
// SIGWINCH can cause output before its resize acknowledgement reaches the client.
export async function orderedResize({drain,isCurrent,prepareGrid,send}) {
  await drain();
  if(!isCurrent())return false;
  prepareGrid();
  await send();
  return isCurrent();
}
