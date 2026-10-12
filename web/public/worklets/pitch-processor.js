// 23.13. AudioWorkletProcessor: pitch shift через кольцевой буфер.
//
// ## Почему без WASM
//
// В AudioWorkletGlobalScope запрещены dynamic import() и fetch().
// Загрузить .wasm можно только вшив его в этот файл на этапе сборки
// (bundled approach). Для учебной главы используем чистый JS —
// ring buffer укладывается в 0.03-0.08 мс на блок при бюджете 2.67 мс,
// то есть в 30 раз меньше. WASM здесь не нужен.
//
// ## Бюджет
//
// process() должен уложиться в 128 / sampleRate секунд. При 48 kHz
// это 2.667 мс. Никаких аллокаций, никаких async, никаких postMessage
// в hot path.
//
// ## Алгоритм
//
// ring[4096]; write идёт со скоростью 1x, read — со скоростью ratio.
// Если read догоняет write — сдвигаем read на R/2 назад.
// Интерполяция линейная между соседними сэмплами.

class PitchProcessor extends AudioWorkletProcessor {
    constructor() {
        super();

        // Кольцевой буфер.
        this.ring = new Float32Array(4096);
        this.write = 0;
        this.read = 2048.0;
        this.ratio = 1.0;
        this.MIN_LAG = 128;

        // Метрики (окно 500 мс).
        this.winMs = 0;
        this.winBlocks = 0;
        this.totalBlocks = 0;
        this.totalXruns = 0;
        this.lastReport = 0;

        // Команды из main thread.
        this.port.onmessage = (e) => {
            const m = e.data || {};
            if (m.type === 'ratio') {
                this.ratio = Math.max(0.5, Math.min(2.0, m.value));
            }
        };

        // Готовы сразу (WASM нет).
        this.port.postMessage({ type: 'ready' });
    }

    process(inputs, outputs) {
        const output = outputs[0];
        const input = inputs[0];
        if (!output || output.length === 0) return true;
        if (!input || input.length === 0) return true;

        const ch = Math.min(input.length, output.length);
        const t0 = performance.now();

        const ring = this.ring;
        const R = ring.length;
        const MIN_LAG = this.MIN_LAG;

        for (let c = 0; c < ch; c++) {
            const inCh = input[c];
            const outCh = output[c];
            const n = Math.min(inCh.length, outCh.length);

            // 1. Запись в ring.
            let w = this.write;
            for (let i = 0; i < n; i++) {
                ring[w] = inCh[i];
                w = (w + 1) % R;
            }
            this.write = w;

            // 2. Чтение с переменной скоростью.
            let r = this.read;
            for (let i = 0; i < n; i++) {
                const i0 = r | 0;               // floor
                const i1 = (i0 + 1) % R;
                const frac = r - i0;
                outCh[i] = ring[i0] * (1.0 - frac) + ring[i1] * frac;
                r += this.ratio;

                // Защита от догоняния write.
                let lag = ((this.write - r) % R + R) % R;
                if (lag < MIN_LAG) {
                    r = this.write - R * 0.5;
                    if (r < 0) r += R;
                }
                if (r >= R) r -= R;
            }
            this.read = r;
        }

        const dt = performance.now() - t0;

        this.winMs += dt;
        this.winBlocks += 1;
        this.totalBlocks += 1;

        const budgetMs = (128 / sampleRate) * 1000;
        if (dt > budgetMs) this.totalXruns += 1;

        const now = performance.now();
        if (now - this.lastReport >= 500 && this.winBlocks > 0) {
            this.lastReport = now;
            this.port.postMessage({
                type: 'stats',
                avgProcessMs: this.winMs / this.winBlocks,
                blocks: this.totalBlocks,
                xruns: this.totalXruns,
            });
            this.winMs = 0;
            this.winBlocks = 0;
        }

        return true;
    }
}

registerProcessor('pitch-processor', PitchProcessor);