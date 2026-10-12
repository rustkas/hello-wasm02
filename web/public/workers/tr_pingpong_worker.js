// 22.3. Ping-pong worker.
//
// Демонстрирует два канала:
//   1. Основной postMessage — для transfer vs copy тестов.
//   2. MessageChannel — приватный канал для ping-pong.
//
// В обоих случаях используется transferable-объекты, чтобы показать
// нулевое копирование.

let channelPort = null;

self.onmessage = (e) => {
    const msg = e.data;

    if (msg.type === "transfer_test") {
        // Echo back с transferable. Буфер, полученный от main thread,
        // передаём обратно с флагом transfer.
        self.postMessage(
            { type: "transfer_ack", t0: msg.t0, size: msg.size },
            [msg.buffer],
        );
        return;
    }

    if (msg.type === "setup_port") {
        channelPort = msg.port;
        channelPort.onmessage = (ev) => {
            const data = ev.data;
            if (data.type === "ping") {
                channelPort.postMessage({
                    type: "pong",
                    seq: data.seq,
                    t0: data.t0,
                });
            }
        };
        // Обязательно после передачи порта.
        channelPort.start();
        return;
    }
};