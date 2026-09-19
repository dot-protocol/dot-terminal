// Accessory arrows must honor the same DECCKM mode as physical xterm keys.
export function accessoryKey(value, applicationCursorKeysMode=false) {
  return applicationCursorKeysMode && /^\x1b\[[ABCD]$/.test(value)
    ? '\x1bO'+value.slice(-1) : value;
}
