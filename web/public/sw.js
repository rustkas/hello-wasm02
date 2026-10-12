// Service Worker: перехват запросов и потоковые ответы.
//
// Три сценария:
// 1. /api/echo — сгенерированный JSON-ответ (из главы 16.13).
// 2. /api/logs/stream — поток NDJSON-логов, генерируется в SW.
// 3. /api/proxy-stream/* — прокси с модификацией ответа на лету.

self.addEventListener('install', () => {
    console.log('[SW] install');
    // Активируем новую версию сразу, не дожидаясь закрытия вкладок.
    self.skipWaiting();
});

self.addEventListener('activate', (event) => {
    console.log('[SW] activate');
    // Берём под контроль уже открытые вкладки без перезагрузки.
    event.waitUntil(self.clients.claim());
});

self.addEventListener('fetch', (event) => {
    const url = new URL(event.request.url);

    // ---- 1. /api/echo: сгенерированный ответ (16.13) ----
    if (url.pathname === '/api/echo') {
        const body = JSON.stringify({
            message: "Ответ сгенерирован Service Worker'ом",
            path: url.pathname,
            method: event.request.method,
            timestamp: Date.now(),
        });

        event.respondWith(
            new Response(body, {
                status: 200,
                headers: { 'Content-Type': 'application/json' },
            })
        );
        return;
    }

    // ---- 2. /api/logs/stream: поток NDJSON, генерируется в SW ----
    if (url.pathname === '/api/logs/stream') {
        const encoder = new TextEncoder();
        const stream = new ReadableStream({
            async start(controller) {
                for (let i = 0; i < 10; i++) {
                    const line = JSON.stringify({
                        seq: i,
                        timestamp: Date.now(),
                        level: i % 3 === 0 ? 'warn' : 'info',
                        message: `Событие номер ${i}`,
                    }) + '\n';
                    controller.enqueue(encoder.encode(line));
                    await new Promise(r => setTimeout(r, 200));
                }
                controller.close();
            },
        });

        event.respondWith(new Response(stream, {
            status: 200,
            headers: {
                'Content-Type': 'application/x-ndjson',
                'X-Generated-By': 'ServiceWorker',
            },
        }));
        return;
    }

    // ---- 3. /api/proxy-stream/*: прокси с модификацией ----
    if (url.pathname.startsWith('/api/proxy-stream/')) {
        // /api/proxy-stream/stream/5 → /api/stream/5
        const target = url.pathname.replace('/api/proxy-stream/', '/api/');

        event.respondWith((async () => {
            try {
                const upstream = await fetch(target);

                // Если у ответа нет тела (204, 304, ошибки) — проксируем как есть.
                if (!upstream.body) {
                    return new Response(await upstream.text(), {
                        status: upstream.status,
                        headers: upstream.headers,
                    });
                }

                const reader = upstream.body.getReader();
                const decoder = new TextDecoder();
                const encoder = new TextEncoder();

                const stream = new ReadableStream({
                    async pull(controller) {
                        const { done, value } = await reader.read();
                        if (done) {
                            controller.close();
                            return;
                        }

                        // Декодируем чанк как поток текста (stream: true).
                        const text = decoder.decode(value, { stream: true });

                        // Добавляем префикс к каждой непустой строке.
                        const modified = text
                            .split('\n')
                            .map(line => line ? `[SW] ${line}` : line)
                            .join('\n');

                        controller.enqueue(encoder.encode(modified));
                    },
                });

                return new Response(stream, {
                    status: upstream.status,
                    headers: upstream.headers,
                });
            } catch (e) {
                console.error('[SW] proxy-stream error:', e);
                return new Response(`Service Worker error: ${e}`, { status: 502 });
            }
        })());
        return;
    }

    // ---- Всё остальное — пропускаем в сеть ----
    event.respondWith(fetch(event.request));
});