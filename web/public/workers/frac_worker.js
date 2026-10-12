// 22.6. Mandelbrot worker.
//
// Динамический import + подробное логирование. Если что-то
// не работает — видно по console.log в DevTools → Sources → Workers.

self.postMessage({ type: "boot", stage: "worker_started" });
console.log("[frac] worker started");

let ready = false;
let wasm_render_chunk = null;
let wasm_instance_id = null;
let canvas = null;
let ctx = null;
let w = 0;
let h = 0;
let active_task_id = 0;
const pending = [];

self.onmessage = (e) => {
    if (!ready) {
        pending.push(e);
        return;
    }
    handle(e.data);
};

self.addEventListener("error", (ev) => {
    console.error("[frac] onerror:", ev.message);
    self.postMessage({ type: "runtime_error", message: ev.message || "?" });
});

self.addEventListener("unhandledrejection", (ev) => {
    console.error("[frac] unhandledrejection:", ev.reason);
    self.postMessage({
        type: "runtime_error",
        message: "unhandledrejection: " + String(ev.reason),
    });
});

(async () => {
    try {
        console.log("[frac] importing wasm module...");
        const mod = await import("/pkg/hello_wasm02.js");
        const keys = Object.keys(mod);
        console.log("[frac] module exports:", keys);

        await mod.default();
        console.log("[frac] init() resolved");

        wasm_instance_id = mod.instance_id;

        // Поддерживаем оба возможных имени — старое и новое.
        wasm_render_chunk =
            mod.render_chunk ||
            mod.render_mandelbrot_chunk ||
            null;
        console.log("[frac] render function:", typeof wasm_render_chunk);

        if (typeof wasm_render_chunk !== "function") {
            throw new Error(
                "no render function in module; exports = " +
                JSON.stringify(keys),
            );
        }
        if (typeof wasm_instance_id !== "function") {
            wasm_instance_id = () => "no-instance-id";
        }

        ready = true;
        const inst = wasm_instance_id();
        console.log("[frac] ready, instance:", inst);
        self.postMessage({ type: "ready", instance: inst });

        while (pending.length) handle(pending.shift().data);
    } catch (err) {
        console.error("[frac] init failed:", err);
        self.postMessage({
            type: "init_error",
            message: String(err) + (err && err.stack ? " | " + err.stack : ""),
        });
    }
})();

function handle(msg) {
    console.log("[frac] handle:", msg.type);
    if (msg.type === "init_canvas") {
        canvas = msg.canvas;
        w = msg.width;
        h = msg.height;
        ctx = canvas.getContext("2d");
        console.log("[frac] init_canvas ok", w, "x", h, "ctx:", !!ctx);
        return;
    }
    if (msg.type === "render") {
        console.log("[frac] render task", msg.task_id);
        render_view(msg).catch((err) => {
            console.error("[frac] render failed:", err);
            self.postMessage({
                type: "runtime_error",
                message: "render: " + String(err),
            });
        });
        return;
    }
}

async function render_view(msg) {
    if (!ctx) {
        throw new Error("no 2d context");
    }
    if (typeof wasm_render_chunk !== "function") {
        throw new Error("no render function");
    }

    const my_task = msg.task_id;
    active_task_id = my_task;

    // Uint8Array — wasm-bindgen для &mut [u8] ожидает именно его.
    const data = new Uint8Array(w * h * 4);
    const t0 = performance.now();

    const CHUNK = 16;
    const total = Math.ceil(h / CHUNK);
    let done = 0;
    let last_report = 0;

    for (let y = 0; y < h; y += CHUNK) {
        if (active_task_id !== my_task) {
            console.log("[frac] preempted at y =", y);
            return;
        }

        const ye = Math.min(y + CHUNK, h);
        const slice = data.subarray(y * w * 4, ye * w * 4);

        wasm_render_chunk(
            slice, w, h, y,
            msg.cx, msg.cy, msg.pixel_size, msg.max_iter, msg.palette,
        );
        done++;

        const now = performance.now();
        if (now - last_report >= 33 || done === total) {
            self.postMessage({ type: "progress", task_id: my_task, done, total });
            last_report = now;
        }

        await new Promise((r) => setTimeout(r, 0));
    }

    if (active_task_id !== my_task) return;

    // ImageData требует Uint8ClampedArray — конвертируем только на выходе.
    const clamped = new Uint8ClampedArray(data.buffer);
    ctx.putImageData(new ImageData(clamped, w, h), 0, 0);

    console.log("[frac] done, elapsed:", performance.now() - t0, "ms");
    self.postMessage({
        type: "result",
        task_id: my_task,
        elapsed_ms: performance.now() - t0,
        instance_id: wasm_instance_id(),
    });
}