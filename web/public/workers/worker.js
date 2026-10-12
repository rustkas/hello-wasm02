// Module worker: pipeline обработки изображений.
// Загружает WASM, ждёт сообщений от главного потока.

import init, { grayscale, box_blur } from '/pkg/hello_wasm02.js';

let ready = false;

init()
    .then(() => {
        ready = true;
        self.postMessage({ type: 'ready' });
    })
    .catch((err) => {
        self.postMessage({ type: 'error', message: String(err) });
    });

self.onmessage = (e) => {
    if (!ready) {
        self.postMessage({ type: 'error', message: 'WASM not ready' });
        return;
    }

    const { filter, width, height, pixels } = e.data;

    // Прогресс: 0% в начале.
    self.postMessage({ type: 'progress', value: 0 });

    const bytes = new Uint8Array(pixels);
    let result;
    try {
        if (filter === 'grayscale') {
            result = grayscale(bytes);
            self.postMessage({ type: 'progress', value: 100 });
        } else if (filter === 'box_blur') {
            result = box_blur(bytes, width, height, 2);
            self.postMessage({ type: 'progress', value: 100 });
        } else {
            self.postMessage({ type: 'error', message: 'unknown filter' });
            return;
        }
    } catch (err) {
        self.postMessage({ type: 'error', message: String(err) });
        return;
    }

    self.postMessage({ type: 'done', value: result });
};