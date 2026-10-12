// 23.12-W. Загрузчик pipeline-worker'а.
//
// Импортирует wasm-pack сборку и вызывает Rust-функцию worker_run(),
// которая регистрирует self.onmessage.

self.postMessage({ type: "boot" });

(async () => {
    try {
        const mod = await import("/pkg/hello_wasm02.js");
        await mod.default();
        if (typeof mod.worker_run !== "function") {
            throw new Error("worker_run not exported from WASM module");
        }
        mod.worker_run();
        self.postMessage({ type: "ready" });
    } catch (err) {
        self.postMessage({
            type: "boot_error",
            message: String(err) + (err && err.stack ? " | " + err.stack : ""),
        });
    }
})();