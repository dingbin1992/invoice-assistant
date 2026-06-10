export function invoke(name, args) {
  return window.__TAURI_INTERNALS__.invoke(name, args);
}
