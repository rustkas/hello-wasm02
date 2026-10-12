// Dedicated Worker: делает fetch и отправляет результат в main thread.
//
// Этот скрипт запускается в отдельном потоке. У него нет DOM,
// нет `window`, но есть `self` (это `DedicatedWorkerGlobalScope`)
// и, что важно, есть `fetch`.

self.addEventListener('message', async (event) => {
    const { id, url } = event.data;

    try {
        const response = await fetch(url);
        const text = await response.text();

        self.postMessage({
            id,
            ok: response.ok,
            status: response.status,
            body: text,
        });
    } catch (error) {
        self.postMessage({
            id,
            ok: false,
            status: 0,
            error: String(error),
        });
    }
});