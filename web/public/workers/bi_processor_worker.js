// 22.6. Background image processor worker.
//
// Worker владеет OffscreenCanvas, всей работой с ним. Main thread
// только шлёт команды и получает прогресс.
//
// ## Динамический импорт
//
// Статический `import ... from "/pkg/..."` Vite обрабатывает и
// выдаёт предупреждение «Files in the public directory are served
// at the root path». Комментарий `@vite-ignore` говорит Vite не
// трогать этот импорт — путь уходит браузеру как есть, без
// трансформации в /public/...

const wasmModule = await import(/* @vite-ignore */ "/pkg/hello_wasm02.js");
const init = wasmModule.default;
const { apply_filter_chunk, instance_id } = wasmModule;

console.log("[bi-worker] imports resolved");
self.postMessage({ type: "boot", stage: "module_loaded" });

console.log("[bi-worker] imports resolved");
self.postMessage({ type: "boot", stage: "module_loaded" });

let ready = false;
let canvas = null;
let ctx = null;
let canvas_w = 0;
let canvas_h = 0;
let source_bytes = null;
const cancel_set = new Set();
const pending = [];

// --- onmessage регистрируем СИНХРОННО ---
// Сообщения до ready уходят в pending.
self.onmessage = (e) => {
    if (!ready) {
        pending.push(e);
        return;
    }
    handle_message(e.data);
};

self.addEventListener("error", (ev) => {
    console.error("[bi-worker] error:", ev.message);
    self.postMessage({
        type: "runtime_error",
        message: ev.message || "(no message)",
    });
});

self.addEventListener("unhandledrejection", (ev) => {
    console.error("[bi-worker] unhandledrejection:", ev.reason);
    self.postMessage({
        type: "runtime_error",
        message: "unhandledrejection: " + String(ev.reason),
    });
});

// --- Таймаут на инициализацию ---
const INIT_TIMEOUT_MS = 10000;
let init_settled = false;
const timeout_id = setTimeout(() => {
    if (!init_settled) {
        console.error("[bi-worker] init timed out after", INIT_TIMEOUT_MS, "ms");
        self.postMessage({
            type: "init_error",
            message:
                "init() не завершился за " + INIT_TIMEOUT_MS +
                " мс. Проверьте DevTools → Network: /pkg/hello_wasm02_bg.wasm " +
                "должен отдаваться со статусом 200 и Content-Type: application/wasm.",
        });
    }
}, INIT_TIMEOUT_MS);

// --- Запуск init с современной сигнатурой ---
console.log("[bi-worker] calling init...");
self.postMessage({ type: "boot", stage: "init_called" });

init({ module_or_path: "/pkg/hello_wasm02_bg.wasm" })
    .then(() => {
        init_settled = true;
        clearTimeout(timeout_id);
        ready = true;

        const id = instance_id();
        console.log("[bi-worker] init resolved, instance:", id);
        self.postMessage({ type: "ready", instance: id });

        // Флаш накопленных сообщений.
        while (pending.length) {
            handle_message(pending.shift().data);
        }
    })
    .catch((err) => {
        init_settled = true;
        clearTimeout(timeout_id);
        console.error("[bi-worker] init failed:", err);
        self.postMessage({
            type: "init_error",
            message: String(err) + (err && err.stack ? " | " + err.stack : ""),
        });
    });

// --- Обработчик сообщений ---

function handle_message(msg) {
    try {
        if (msg.type === "init_canvas") {
            canvas = msg.canvas;
            canvas_w = msg.width;
            canvas_h = msg.height;
            ctx = canvas.getContext("2d");
            console.log("[bi-worker] init_canvas ok", canvas_w, "x", canvas_h);
            return;
        }
        if (msg.type === "generate_sample") {
            generate_sample();
            return;
        }
        if (msg.type === "source_file") {
            handle_source_file(msg).catch((err) => {
                self.postMessage({
                    type: "runtime_error",
                    message: "source_file: " + String(err),
                });
            });
            return;
        }
        if (msg.type === "cancel") {
            cancel_set.add(msg.task_id);
            return;
        }
        if (msg.type === "process") {
            if (!source_bytes) {
                self.postMessage({
                    type: "runtime_error",
                    message: "process: нет исходника — нажмите «Пример» или выберите файл",
                });
                return;
            }
            handle_process(msg).catch((err) => {
                self.postMessage({
                    type: "runtime_error",
                    message: "process: " + String(err),
                });
            });
            return;
        }
        if (msg.type === "download") {
            handle_download().catch((err) => {
                self.postMessage({
                    type: "runtime_error",
                    message: "download: " + String(err),
                });
            });
            return;
        }
    } catch (err) {
        self.postMessage({
            type: "runtime_error",
            message: "handle_message: " + String(err),
        });
    }
}

function generate_sample() {
    if (!ctx) {
        console.warn("[bi-worker] generate_sample: no ctx");
        return;
    }
    const w = canvas_w;
    const h = canvas_h;

    const grad = ctx.createLinearGradient(0, 0, w, h);
    grad.addColorStop(0, "#0d1117");
    grad.addColorStop(1, "#1f6feb");
    ctx.fillStyle = grad;
    ctx.fillRect(0, 0, w, h);

    const colors = ["#f0883e", "#3fb950", "#a371f7", "#f0f6fc"];
    for (let i = 0; i < colors.length; i++) {
        const cx = 90 + i * 100;
        const cy = 120 + (i % 2) * 100;
        const r = 40 + i * 8;
        ctx.fillStyle = colors[i];
        ctx.beginPath();
        ctx.arc(cx, cy, r, 0, Math.PI * 2);
        ctx.fill();
    }

    for (let i = 0; i < 5; i++) {
        const y = 20 + i * 70;
        ctx.fillStyle = i % 2 === 0 ? "#d2a8ff" : "#56d4dd";
        ctx.fillRect(10, y, w - 20, 6);
    }

    ctx.fillStyle = "#f0f6fc";
    ctx.font = "28px sans-serif";
    ctx.fillText("22.6 — image processor", 30, h - 30);

    const img = ctx.getImageData(0, 0, w, h);
    source_bytes = new Uint8Array(img.data);
    console.log("[bi-worker] generate_sample ok, size:", source_bytes.length);
    self.postMessage({ type: "source_ready", size: source_bytes.length });
}

async function handle_source_file(msg) {
    if (!ctx) return;
    const buffer = new Uint8Array(msg.buffer);
    const mime = msg.mime || "image/png";
    const blob = new Blob([buffer], { type: mime });
    const bitmap = await createImageBitmap(blob);

    ctx.fillStyle = "#0d1117";
    ctx.fillRect(0, 0, canvas_w, canvas_h);
    ctx.drawImage(bitmap, 0, 0, canvas_w, canvas_h);
    bitmap.close();

    const img = ctx.getImageData(0, 0, canvas_w, canvas_h);
    source_bytes = new Uint8Array(img.data);
    self.postMessage({ type: "source_ready", size: source_bytes.length });
}

async function handle_process(msg) {
    if (!ctx || !source_bytes) return;
    const { task_id, filter, passes } = msg;

    const work = new Uint8Array(source_bytes);
    const t0 = performance.now();

    const CHUNK_ROWS = 8;
    const total_steps = passes * Math.ceil(canvas_h / CHUNK_ROWS);
    let done_steps = 0;
    let last_report_ms = 0;

    for (let p = 0; p < passes; p++) {
        for (let y = 0; y < canvas_h; y += CHUNK_ROWS) {
            if (cancel_set.has(task_id)) {
                cancel_set.delete(task_id);
                draw_bytes(source_bytes, canvas_w, canvas_h);
                self.postMessage({ type: "cancelled", task_id });
                return;
            }

            const y_end = Math.min(y + CHUNK_ROWS, canvas_h);
            apply_filter_chunk(work, canvas_w, canvas_h, y, y_end, filter);
            done_steps++;

            const now = performance.now();
            if (now - last_report_ms >= 33 || done_steps === total_steps) {
                self.postMessage({
                    type: "progress",
                    task_id,
                    done: done_steps,
                    total: total_steps,
                });
                last_report_ms = now;
            }

            await new Promise((r) => setTimeout(r, 0));
        }
    }

    draw_bytes(work, canvas_w, canvas_h);
    const elapsed_ms = performance.now() - t0;
    self.postMessage({
        type: "result",
        task_id,
        elapsed_ms,
        instance_id: instance_id(),
    });
}

function draw_bytes(bytes, w, h) {
    if (!ctx) return;
    const clamped = new Uint8ClampedArray(bytes);
    const img = new ImageData(clamped, w, h);
    ctx.putImageData(img, 0, 0);
}

async function handle_download() {
    if (!canvas) return;
    const blob = await canvas.convertToBlob({ type: "image/png" });
    const buffer = await blob.arrayBuffer();
    self.postMessage(
        {
            type: "download_data",
            buffer: buffer,
            mime: "image/png",
        },
        [buffer],
    );
}