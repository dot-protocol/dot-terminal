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

// Ordered output frames. A `read_frame` reply labels its bytes with the grid they were produced
// on, a geometry epoch and the stream's incarnation, so a view never has to sample the screen
// beside the byte stream (the old race, and ~30 full snapshots a second per follower).

/** Does this failure mean "this keeper or backend predates read_frame"? Then, and only then, fall back. */
export function framesUnsupported(error) {
 if(error?.status===422||error?.status===400)return true; // an older backend cannot decode the operation
 return error?.code==='keeper-error'&&/unknown variant/.test(String(error.message)); // an older keeper
}

/**
 * What a view must do BEFORE parsing a frame's bytes.
 *  - a different incarnation is a different stream: offsets mean nothing, start again from 0;
 *  - a follower adopts the frame's grid, so bytes are always parsed on the grid they were made for;
 *  - the controller never adopts: it set the grid itself, and old-grid bytes still in flight must
 *    not bounce its window back.
 */
export function framePlan({frame,knownIncarnation,controller,cols,rows}) {
 if(knownIncarnation&&frame.incarnation!==knownIncarnation)return {restart:true,resize:null};
 const differs=frame.cols!==cols||frame.rows!==rows;
 return {restart:false,resize:!controller&&differs&&frame.cols>0&&frame.rows>0?{cols:frame.cols,rows:frame.rows}:null};
}
