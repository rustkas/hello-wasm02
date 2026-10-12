// 23.8-W. MediaStream pipeline в Dedicated Worker.
//
// Загружается как модульный worker:
//   new Worker('workers/ms_worker.js', { type: 'module' })
//
// ## Два режима запуска
//
// 1. `{ type: 'start', track }` — трек передан из main thread.
//    Работает в Chrome/Edge. В Firefox падает с DataCloneError
//    ещё на этапе postMessage — main thread ловит и предлагает
//    «Start synthetic».
//
// 2. `{ type: 'start-synthetic', width, height }` — worker сам
//    создаёт OffscreenCanvas, анимирует и вызывает captureStream().
//    Работает во всех трёх движках: трек не покидает worker.

import init, { process_grayscale, worker_ready } from '../pkg/hello_wasm02.js';

let initialized = false;

async function ensureInit() {
    if (!initialized) {
        await init();
        if (!worker_ready()) throw new Error('worker_ready() returned false');
        initialized = true;
    }
}

/// Общий pipeline: processor.readable → WASM grayscale → generator.writable.
/// Возвращает `outputTrack` (уже отправленный в main thread).
function runPipeline(processor, generator) {
    const reader = processor.readable.getReader();
    const writer = generator.writable.getWriter();

    let frameCount = 0;
    let lastFpsTime = performance.now();
    let lastFpsFrames = 0;
    let totalProcessMs = 0;

    (async () => {
        try {
            while (true) {
                const { value: frame, done } = await reader.read();
                if (done) break;

                const w = frame.displayWidth;
                const h = frame.displayHeight;
                if (w === 0 || h === 0) { frame.close(); continue; }

                const t0 = performance.now();

                const buffer = new Uint8Array(w * h * 4);
                try { await frame.copyTo(buffer, { format: 'RGBA' }); }
                catch (e) { frame.close(); continue; }

                let processed;
                try { processed = process_grayscale(buffer, w, h); }
                catch (e) { frame.close(); continue; }

                let bitmap;
                try {
                    const imageData = new ImageData(new Uint8ClampedArray(processed), w, h);
                    bitmap = await createImageBitmap(imageData);
                } catch (e) { frame.close(); continue; }

                let newFrame;
                try {
                    newFrame = new VideoFrame(bitmap, {
                        timestamp: frame.timestamp,
                        duration: frame.duration ?? 33333,
                    });
                } catch (e) { frame.close(); bitmap.close(); continue; }

                frame.close();
                bitmap.close();

                const t1 = performance.now();
                totalProcessMs += t1 - t0;

                try { await writer.write(newFrame); }
                catch (e) { break; }
                frameCount++;

                if (frameCount % 15 === 0) {
                    const now = performance.now();
                    const dt = (now - lastFpsTime) / 1000;
                    const df = frameCount - lastFpsFrames;
                    lastFpsTime = now;
                    lastFpsFrames = frameCount;
                    self.postMessage({
                        type: 'stats',
                        fps: dt > 0 ? df / dt : 0,
                        frames: frameCount,
                        avgProcessMs: totalProcessMs / frameCount,
                        frameSize: `${w}×${h}`,
                    });
                }
            }
        } catch (e) {
            self.postMessage({ type: 'error', error: 'pipeline: ' + e });
        } finally {
            try { reader.cancel(); } catch (_) { }
            try { writer.close(); } catch (_) { }
        }
    })();
}

/// Создаёт VideoTrackGenerator (или MediaStreamTrackGenerator) и
/// возвращает { generator, outputTrack }.
function makeGenerator() {
    if (typeof VideoTrackGenerator !== 'undefined') {
        const g = new VideoTrackGenerator();
        return { generator: g, outputTrack: g.track };
    }
    if (typeof MediaStreamTrackGenerator !== 'undefined') {
        const g = new MediaStreamTrackGenerator({ kind: 'video' });
        return { generator: g, outputTrack: g };
    }
    throw new Error('Ни VideoTrackGenerator, ни MediaStreamTrackGenerator');
}

/// Рисует кадр синтетической сцены на OffscreenCanvas.
/// Та же анимация, что была в main thread.
function drawSynthFrame(ctx, w, h, t) {
    const hue = (t * 60) % 360;
    const grad = ctx.createLinearGradient(0, 0, w, h);
    grad.addColorStop(0, `hsl(${hue}, 70%, 50%)`);
    grad.addColorStop(1, `hsl(${(hue + 90) % 360}, 70%, 40%)`);
    ctx.fillStyle = grad;
    ctx.fillRect(0, 0, w, h);

    const cx = w / 2 + Math.sin(t * 1.5) * 100;
    const cy = h / 2 + Math.cos(t * 1.5) * 80;
    ctx.fillStyle = '#f0f6fc';
    ctx.beginPath();
    ctx.arc(cx, cy, 36, 0, Math.PI * 2);
    ctx.fill();
}

self.onmessage = async (event) => {
    const msg = event.data;
    if (msg.type !== 'start' && msg.type !== 'start-synthetic') return;

    if (typeof MediaStreamTrackProcessor === 'undefined') {
        self.postMessage({
            type: 'error',
            error: 'MediaStreamTrackProcessor недоступен в этом worker.',
        });
        return;
    }

    try {
        await ensureInit();
    } catch (e) {
        self.postMessage({ type: 'error', error: 'WASM init: ' + e });
        return;
    }

    // ---- Получаем inputTrack ----
    let inputTrack;

    if (msg.type === 'start') {
        // Трек передан из main thread (Chrome/Edge).
        inputTrack = msg.track;
    } else {
        // start-synthetic: создаём OffscreenCanvas внутри worker'а
        // и генерируем трек здесь же. Трек не покидает worker,
        // поэтому Firefox-ограничение на передачу не срабатывает.
        const w = msg.width || 480;
        const h = msg.height || 360;
        const canvas = new OffscreenCanvas(w, h);
        const ctx = canvas.getContext('2d');

        let rafId = null;
        const started = performance.now();
        const draw = (now) => {
            drawSynthFrame(ctx, w, h, (now - started) / 1000);
            rafId = self.requestAnimationFrame(draw);
        };
        rafId = self.requestAnimationFrame(draw);

        const stream = canvas.captureStream(30);
        inputTrack = stream.getVideoTracks()[0];
        if (!inputTrack) {
            self.postMessage({ type: 'error', error: 'captureStream: no track' });
            return;
        }
        // Сохраняем rafId, чтобы отменить при остановке (см. ниже).
        // Для демо достаточно: при terminate worker'а rAF умрёт сам.
    }

    // ---- Processor ----
    let processor;
    try {
        processor = new MediaStreamTrackProcessor({ track: inputTrack });
    } catch (e) {
        self.postMessage({ type: 'error', error: 'MediaStreamTrackProcessor: ' + e });
        return;
    }

    // ---- Generator ----
    let generator, outputTrack;
    try {
        ({ generator, outputTrack } = makeGenerator());
    } catch (e) {
        self.postMessage({ type: 'error', error: 'generator: ' + e });
        return;
    }

    // Отправляем outputTrack в main thread.
    self.postMessage({ type: 'track', track: outputTrack }, [outputTrack]);

    // ---- Pipeline ----
    runPipeline(processor, generator);
};