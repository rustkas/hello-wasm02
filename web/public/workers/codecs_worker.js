// 23. WebCodecs worker.
//
// Два режима обработки пикселей:
//   - "cpu": VideoFrame → copyTo → Rust invert → ImageData → VideoFrame
//   - "gpu": VideoFrame → texImage2D → WebGL2 invert shader → ImageBitmap
//
// В обоих режимах ресурсы (canvas, контекст, буфер) переиспользуются
// между кадрами — никаких аллокаций в цикле.
//
// ## Флип по Y
//
// WebGL читает текстуру снизу вверх, а VideoFrame содержит пиксели
// сверху вниз. Без UNPACK_FLIP_Y_WEBGL картинка переворачивается.
//
// ## Почему ImageBitmap в GPU-режиме
//
// gl_canvas после drawArrays содержит результат. transferToImageBitmap()
// забирает его в GPU-resident ImageBitmap за O(1) — без readback.
// Из 30 таких bitmap'ов собирается корректный поток кадров для
// re-encode, а не 30 копий последнего кадра.

self.postMessage({ type: "boot", stage: "worker_started" });
console.log("[codecs] worker started");

// =============================================================================
// Состояние
// =============================================================================

let ready = false;
let wasm_invert_rgba = null;
let wasm_instance_id = null;

let result_canvas = null;
let result_ctx = null;
let canvas_w = 0;
let canvas_h = 0;

// CPU-путь: переиспользуемый canvas + буфер.
let work_canvas = null;
let work_ctx = null;
let rgba_buffer = null;

// GPU-путь: переиспользуемый canvas + WebGL2.
let gl_canvas = null;
let gl = null;
let gl_program = null;
let gl_texture = null;

let task_codec = "avc1.42001E";
let task_w = 0;
let task_h = 0;
let task_frame_count = 0;
let task_mode = "cpu";
let task_chunks = [];
let decoder_config = null;

const pending = [];

// =============================================================================
// onmessage — синхронно, до import
// =============================================================================

self.onmessage = (e) => {
    if (!ready) { pending.push(e); return; }
    handle(e.data);
};

self.addEventListener("error", (ev) => {
    self.postMessage({ type: "runtime_error", message: ev.message || "?" });
});

self.addEventListener("unhandledrejection", (ev) => {
    self.postMessage({
        type: "runtime_error",
        message: "unhandledrejection: " + String(ev.reason),
    });
});

// =============================================================================
// WASM
// =============================================================================

(async () => {
    try {
        const mod = await import("/pkg/hello_wasm02.js");
        await mod.default();
        wasm_invert_rgba = mod.invert_rgba;
        wasm_instance_id = mod.instance_id;
        if (typeof wasm_invert_rgba !== "function") {
            throw new Error("no invert_rgba in module");
        }
        ready = true;
        const inst = typeof wasm_instance_id === "function"
            ? wasm_instance_id() : "wasm-unknown";
        self.postMessage({ type: "ready", instance: inst });
        while (pending.length) handle(pending.shift().data);
    } catch (err) {
        self.postMessage({
            type: "init_error",
            message: String(err) + (err?.stack ? " | " + err.stack : ""),
        });
    }
})();

// =============================================================================
// Обработчик сообщений
// =============================================================================

function handle(msg) {
    switch (msg.type) {
        case "init_canvas":
            result_canvas = msg.canvas;
            canvas_w = msg.width;
            canvas_h = msg.height;
            result_ctx = result_canvas.getContext("2d");
            break;

        case "decoder_config":
            decoder_config = msg.config;
            break;

        case "start":
            task_codec = msg.codec || task_codec;
            task_w = msg.width || 0;
            task_h = msg.height || 0;
            task_frame_count = msg.frameCount || 0;
            task_mode = msg.mode || "cpu";
            task_chunks = [];
            console.log("[codecs] start task:", task_codec,
                task_w, "x", task_h, "mode:", task_mode);
            break;

        case "encoded_chunk":
            task_chunks.push(msg);
            break;

        case "finish":
            process_task().catch((err) => {
                console.error("[codecs] process failed:", err);
                self.postMessage({
                    type: "runtime_error",
                    message: "process: " + String(err),
                });
            });
            break;
    }
}

// =============================================================================
// Переиспользуемые ресурсы
// =============================================================================

function ensure_cpu_canvas() {
    if (work_canvas) return;
    work_canvas = new OffscreenCanvas(task_w, task_h);
    work_ctx = work_canvas.getContext("2d");
    rgba_buffer = new Uint8ClampedArray(task_w * task_h * 4);
    console.log("[codecs] cpu canvas ready", task_w, "x", task_h);
}

function ensure_gpu_canvas() {
    if (gl) return;
    gl_canvas = new OffscreenCanvas(task_w, task_h);
    gl = gl_canvas.getContext("webgl2", { premultipliedAlpha: false });
    if (!gl) throw new Error("WebGL2 not available in worker");

    // WebGL читает текстуры снизу вверх; VideoFrame — сверху вниз.
    // Без этого флага картинка переворачивается вертикально.
    gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, true);

    const vs = `#version 300 es
        in vec2 a_pos;
        out vec2 v_uv;
        void main() {
            v_uv = a_pos * 0.5 + 0.5;
            gl_Position = vec4(a_pos, 0.0, 1.0);
        }`;

    const fs = `#version 300 es
        precision mediump float;
        in vec2 v_uv;
        uniform sampler2D u_tex;
        out vec4 out_color;
        void main() {
            vec4 c = texture(u_tex, v_uv);
            out_color = vec4(1.0 - c.rgb, c.a);
        }`;

    const compile = (type, src) => {
        const s = gl.createShader(type);
        gl.shaderSource(s, src);
        gl.compileShader(s);
        if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) {
            throw new Error(gl.getShaderInfoLog(s));
        }
        return s;
    };

    const prog = gl.createProgram();
    gl.attachShader(prog, compile(gl.VERTEX_SHADER, vs));
    gl.attachShader(prog, compile(gl.FRAGMENT_SHADER, fs));
    gl.linkProgram(prog);
    if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) {
        throw new Error(gl.getProgramInfoLog(prog));
    }
    gl_program = prog;

    const quad = new Float32Array([
        -1, -1, 1, -1, -1, 1,
        -1, 1, 1, -1, 1, 1,
    ]);
    const buf = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, buf);
    gl.bufferData(gl.ARRAY_BUFFER, quad, gl.STATIC_DRAW);

    const pos_loc = gl.getAttribLocation(prog, "a_pos");
    gl.enableVertexAttribArray(pos_loc);
    gl.vertexAttribPointer(pos_loc, 2, gl.FLOAT, false, 0, 0);

    gl_texture = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, gl_texture);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);

    gl.viewport(0, 0, task_w, task_h);
    console.log("[codecs] gpu canvas + webgl2 ready");
}

// =============================================================================
// Обработка одного кадра
// =============================================================================

/// CPU: VideoFrame → copyTo → Rust invert → ImageData.
function process_cpu(frame) {
    frame.copyTo(rgba_buffer, { format: "RGBA" });
    const u8 = new Uint8Array(
        rgba_buffer.buffer,
        rgba_buffer.byteOffset,
        rgba_buffer.byteLength,
    );
    wasm_invert_rgba(u8);
    return new ImageData(rgba_buffer, task_w, task_h);
}

/// GPU: VideoFrame → texImage2D → invert shader → canvas.
///
/// Возвращает ImageBitmap — GPU-resident копию содержимого canvas,
/// полученную через transferToImageBitmap (без readback).
function process_gpu(frame) {
    gl.bindTexture(gl.TEXTURE_2D, gl_texture);
    gl.texImage2D(
        gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, frame,
    );
    gl.useProgram(gl_program);
    gl.drawArrays(gl.TRIANGLES, 0, 6);
    return gl_canvas.transferToImageBitmap();
}

// =============================================================================
// Основной пайплайн
// =============================================================================

async function process_task() {
    if (!result_ctx) throw new Error("no result canvas");
    if (task_chunks.length === 0) throw new Error("no chunks received");

    if (task_mode === "gpu") {
        ensure_gpu_canvas();
    } else {
        ensure_cpu_canvas();
    }

    self.postMessage({
        type: "phase",
        name: `decode → ${task_mode} invert → re-encode`,
    });

    // --- Декодер ---
    const decoded_frames = [];
    const decoder = new VideoDecoder({
        output: (frame) => decoded_frames.push(frame),
        error: (e) => console.error("[codecs] decode error:", e),
    });

    const cfg = decoder_config || {
        codec: task_codec,
        codedWidth: task_w,
        codedHeight: task_h,
    };
    decoder.configure(cfg);

    const decode_start = performance.now();
    for (const c of task_chunks) {
        decoder.decode(new EncodedVideoChunk({
            type: c.chunkType,
            timestamp: c.timestamp,
            duration: c.duration || 0,
            data: new Uint8Array(c.buffer),
        }));
    }
    await decoder.flush();
    decoder.close();
    const decode_ms = performance.now() - decode_start;
    console.log("[codecs] decoded", decoded_frames.length,
        "frames in", decode_ms.toFixed(1), "ms");

    if (decoded_frames.length === 0) throw new Error("no frames decoded");

    // --- Инверсия ---
    self.postMessage({
        type: "progress",
        done: 0,
        total: decoded_frames.length,
    });

    const invert_start = performance.now();

    // cpu_frames: массив ImageData.
    // gpu_frames: массив ImageBitmap (GPU-resident, без readback).
    const cpu_frames = [];
    const gpu_frames = [];

    for (let i = 0; i < decoded_frames.length; i++) {
        const frame = decoded_frames[i];

        if (task_mode === "gpu") {
            const bitmap = process_gpu(frame);
            gpu_frames.push(bitmap);
        } else {
            const img = process_cpu(frame);
            cpu_frames.push(img);
        }
        frame.close();

        self.postMessage({
            type: "progress",
            done: i + 1,
            total: decoded_frames.length,
        });
        await new Promise((r) => setTimeout(r, 0));
    }

    const invert_ms = performance.now() - invert_start;
    console.log(`[codecs] ${task_mode} invert:`, invert_ms.toFixed(1), "ms");

    // --- Показ последнего кадра на result_canvas ---
    if (task_mode === "gpu" && gpu_frames.length > 0) {
        const last = gpu_frames[gpu_frames.length - 1];
        result_ctx.drawImage(last, 0, 0, canvas_w, canvas_h);
    } else if (cpu_frames.length > 0) {
        const last = cpu_frames[cpu_frames.length - 1];
        if (last.width === canvas_w && last.height === canvas_h) {
            result_ctx.putImageData(last, 0, 0);
        } else {
            const tmp = new OffscreenCanvas(last.width, last.height);
            tmp.getContext("2d").putImageData(last, 0, 0);
            result_ctx.drawImage(tmp, 0, 0, canvas_w, canvas_h);
        }
    }

    // --- Re-encode ---
    self.postMessage({ type: "phase", name: "re-encode" });

    let reencoded_count = 0;
    const reencode_start = performance.now();
    const encoder = new VideoEncoder({
        output: () => { reencoded_count++; },
        error: (e) => console.error("[codecs] encode error:", e),
    });
    encoder.configure({
        codec: task_codec,
        width: canvas_w,
        height: canvas_h,
        bitrate: 1_000_000,
        framerate: 30,
    });

    if (task_mode === "gpu") {
        // Каждый ImageBitmap — GPU-resident. VideoFrame(bitmap) идёт
        // напрямую в encoder без спуска на CPU.
        for (let i = 0; i < gpu_frames.length; i++) {
            const bitmap = gpu_frames[i];
            const vf = new VideoFrame(bitmap, { timestamp: i * 33333 });
            encoder.encode(vf, { keyFrame: i === 0 });
            vf.close();
            bitmap.close();
            await new Promise((r) => setTimeout(r, 0));
        }
    } else {
        // CPU-путь: putImageData на переиспользуемый canvas → VideoFrame.
        for (let i = 0; i < cpu_frames.length; i++) {
            work_ctx.putImageData(cpu_frames[i], 0, 0);
            const vf = new VideoFrame(work_canvas, { timestamp: i * 33333 });
            encoder.encode(vf, { keyFrame: i === 0 });
            vf.close();
            await new Promise((r) => setTimeout(r, 0));
        }
    }

    await encoder.flush();
    encoder.close();
    const reencode_ms = performance.now() - reencode_start;
    console.log("[codecs] re-encoded", reencoded_count,
        "frames in", reencode_ms.toFixed(1), "ms");

    self.postMessage({
        type: "timings",
        decode_ms: decode_ms,
        rust_ms: invert_ms,
        reencode_ms: reencode_ms,
    });
    self.postMessage({ type: "done" });

    task_chunks = [];
    decoder_config = null;
}