// Service Worker — один на origin.
// Обслуживает:
//   • offline-first (глава 20): cache-first / network-first.
//   • клик по SW-уведомлению (глава 33): notificationclick.
//   • закрытие SW-уведомления (глава 33): notificationclose.
//
// Один scope `/` — второй SW в том же scope невозможен.

const CACHE_APP_SHELL = 'app-shell-v1';
const CACHE_RUNTIME = 'runtime-v1';

const ASSETS = [
    '/',
    '/index.html',
    '/pkg/app.js',
    '/pkg/app_bg.wasm',
];

// ============================================================
//                    INSTALL / ACTIVATE
// ============================================================

self.addEventListener('install', (event) => {
    event.waitUntil(
        caches.open(CACHE_APP_SHELL).then((cache) => cache.addAll(ASSETS))
    );
    self.skipWaiting();
});

self.addEventListener('activate', (event) => {
    event.waitUntil(
        caches.keys().then((keys) =>
            Promise.all(
                keys
                    .filter((k) => k !== CACHE_APP_SHELL && k !== CACHE_RUNTIME)
                    .map((k) => caches.delete(k))
            )
        )
    );
    self.clients.claim();
});

// ============================================================
//                    FETCH (offline-first)
// ============================================================

self.addEventListener('fetch', (event) => {
    const url = new URL(event.request.url);

    if (url.pathname.startsWith('/api/')) {
        event.respondWith(networkFirst(event.request));
        return;
    }

    event.respondWith(cacheFirst(event.request));
});

async function cacheFirst(request) {
    const cached = await caches.match(request);
    if (cached) return cached;
    const response = await fetch(request);
    if (response.ok) {
        const cache = await caches.open(CACHE_APP_SHELL);
        cache.put(request, response.clone());
    }
    return response;
}

async function networkFirst(request) {
    try {
        const response = await fetch(request);
        if (response.ok) {
            const cache = await caches.open(CACHE_RUNTIME);
            cache.put(request, response.clone());
        }
        return response;
    } catch {
        const cached = await caches.match(request);
        if (cached) return cached;
        throw new Error('offline: нет ни сети, ни кэша');
    }
}

// ============================================================
//                    NOTIFICATIONCLICK (глава 33)
// ============================================================
//
// Срабатывает, когда пользователь кликает по SW-уведомлению.
// Контекст: SW, не страница. Это ключевое отличие от page-уведомлений —
// там клик приходит в `notification.onclick` страницы.
//
// Задача: закрыть попап, отправить сообщение всем открытым вкладкам,
// сфокусировать существующую или открыть новую.

self.addEventListener('notificationclick', (event) => {
    event.notification.close();

    const tag = event.notification.tag || '';

    event.waitUntil((async () => {
        const clients = await self.clients.matchAll({
            type: 'window',
            includeUncontrolled: true,
        });

        // Оповещаем страницы о клике. Это то, чего нет у page-уведомлений:
        // событие приходит в SW и только потом — во вкладки.
        for (const client of clients) {
            client.postMessage({ type: 'notification-click', tag });
        }

        // Фокусируем первую вкладку или открываем новую.
        for (const client of clients) {
            if ('focus' in client) {
                return client.focus();
            }
        }
        if (self.clients.openWindow) {
            return self.clients.openWindow('/');
        }
    })());
});

// ============================================================
//                    NOTIFICATIONCLOSE (глава 33)
// ============================================================
//
// Пользователь закрыл попап (крестик). Тоже событие SW, не страницы.

self.addEventListener('notificationclose', (event) => {
    const tag = event.notification.tag || '';
    event.waitUntil((async () => {
        const clients = await self.clients.matchAll({
            type: 'window',
            includeUncontrolled: true,
        });
        for (const client of clients) {
            client.postMessage({ type: 'notification-close', tag });
        }
    })());
});