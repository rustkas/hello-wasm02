// 22.5. Stripe worker — module worker для пула.
//
// Загружает тот же wasm-pack-пакет, что и main thread. Каждый
// экземпляр пула имеет свой WASM-инстанс. Worker обрабатывает одну
// горизонтальную полосу и возвращает результат через transferable.

import init, { box_blur_rgba, instance_id } from "/pkg/hello_wasm02.js";

let ready = false;

init()
    .then(() => {
        ready = true;
        self.postMessage({ type: "ready", instance: instance_id() });
    })
    .catch((err) => {
        self.postMessage({ type: "init_error", message: String(err) });
    });

self.onmessage = (e) => {
    if (!ready) return;
    const msg = e.data;
    if (msg.type !== "stripe") return;

    const input = new Uint8Array(msg.buffer);
    const t0 = performance.now();
    const output = box_blur_rgba(input, msg.width, msg.height, msg.passes);
    const t1 = performance.now();

    // output — view над wasm-памятью. Копируем в JS-heap для transfer.
    const copy = new Uint8Array(output);

    self.postMessage(
        {
            type: "stripe_done",
            buffer: copy.buffer,
            y: msg.y,
            width: msg.width,
            height: msg.height,
            elapsed_ms: t1 - t0,
            slot: msg.slot,
        },
        [copy.buffer],
    );
};