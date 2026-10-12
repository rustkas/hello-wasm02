// WASM-модуль в web/pkg/ — Vite обрабатывает его как source.
const mod = await import("/pkg/hello_wasm02.js");
await mod.default();
console.log("WASM module initialized");