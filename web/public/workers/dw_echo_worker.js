// 22.2. Echo worker.
//
// Демонстрирует базовые примитивы DedicatedWorkerGlobalScope:
// self.onmessage, self.postMessage, self.close.
// Не использует OffscreenCanvas — фокус на интерфейсе Worker'а.

self.onmessage = (e) => {
    const msg = e.data;

    if (msg.type === "echo") {
        self.postMessage({
            type: "result",
            text: String(msg.text).toUpperCase(),
        });
        return;
    }

    if (msg.type === "throw") {
        // Необработанное исключение — Worker будет терминирован,
        // main thread получит ErrorEvent.
        throw new Error("Намеренная ошибка из Worker'а");
    }

    if (msg.type === "close") {
        self.close();
        return;
    }
};