// 22.4. Filter worker — module worker.
//
// Загружает тот же wasm-pack-пакет, что и main thread, но во втором
// JS-контексте. Это создаёт ВТОРУЮ инстанциацию WASM-модуля:
// своя линейная память, свой WebAssembly.Instance, свой thread_local!.
//
// Ключевое: { type: "module" } при создании — обязательное условие
// для import(). Без него этот файл не распарсится.

import init, { box_blur_rgba, instance_id } from "/pkg/hello_wasm02.js";

let ready = false;

init()
    .then(() => {
        ready = true;
        // Сообщаем main thread свой ID — доказательство второй инстанциации.
        self.postMessage({ type: "ready", instance: instance_id() });
    })
    .catch((err) => {
        self.postMessage({ type: "init_error", message: String(err) });
    });

self.onmessage = (e) => {
    if (!ready) return;
    const msg = e.data;
    if (msg.type !== "filter") return;

    const input = new Uint8Array(msg.buffer);
    const t0 = performance.now();
    const output = box_blur_rgba(input, msg.width, msg.height, msg.passes);
    const t1 = performance.now();

    // `output` — view над wasm-памятью Worker-инстанса. Трансферить
    // нельзя — копируем в свежий JS-heap ArrayBuffer.
    const copy = new Uint8Array(output);

    self.postMessage(
        {
            type: "result",
            buffer: copy.buffer,
            width: msg.width,
            height: msg.height,
            elapsed_ms: t1 - t0,
            instance_id: instance_id(),
        },
        [copy.buffer],
    );
};