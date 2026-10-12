// 22. Particle worker.
//
// Worker владеет OffscreenCanvas и анимирует частицы. Общается с main
// thread через postMessage: принимает команды (init, palette, count,
// pause, resume, transfer_test), отправляет статистику (fps) и
// подтверждения (transfer_ack).

let canvas = null;
let ctx = null;
let width = 0;
let height = 0;
let running = true;
let palette = ['#58a6ff', '#3fb950', '#f0883e', '#d2a8ff'];
let targetCount = 200;
let particles = [];
let t0 = 0;
let lastFpsTime = 0;
let frames = 0;
let lastFrameTime = 0;

function makeParticles(n) {
    const arr = new Array(n);
    for (let i = 0; i < n; i++) {
        arr[i] = {
            x: Math.random() * width,
            y: Math.random() * height,
            vx: (Math.random() - 0.5) * 2,
            vy: (Math.random() - 0.5) * 2,
            r: 1 + Math.random() * 3,
            c: Math.floor(Math.random() * palette.length),
        };
    }
    return arr;
}

function tick(now) {
    requestAnimationFrame(tick);
    if (!ctx) return;

    if (!lastFrameTime) lastFrameTime = now;
    const dt = Math.min((now - lastFrameTime) / 16.667, 4.0);
    lastFrameTime = now;

    if (!running) {
        // Всё равно обновляем статистику — так main thread видит, что
        // Worker жив, просто на паузе.
        maybeSendFps(now);
        return;
    }

    // Убедимся, что число частиц соответствует цели.
    if (particles.length !== targetCount) {
        particles = makeParticles(targetCount);
    }

    // Очистка.
    ctx.fillStyle = '#0d1117';
    ctx.fillRect(0, 0, width, height);

    // Движение и отрисовка.
    for (let i = 0; i < particles.length; i++) {
        const p = particles[i];
        p.x += p.vx * dt;
        p.y += p.vy * dt;

        if (p.x < 0 || p.x > width) { p.vx = -p.vx; p.x = Math.max(0, Math.min(width, p.x)); }
        if (p.y < 0 || p.y > height) { p.vy = -p.vy; p.y = Math.max(0, Math.min(height, p.y)); }

        ctx.fillStyle = palette[p.c % palette.length];
        ctx.beginPath();
        ctx.arc(p.x, p.y, p.r, 0, Math.PI * 2);
        ctx.fill();
    }

    frames++;
    maybeSendFps(now);
}

function maybeSendFps(now) {
    if (!lastFpsTime) lastFpsTime = now;
    const elapsed = now - lastFpsTime;
    if (elapsed >= 500) {
        const fps = (frames * 1000) / elapsed;
        self.postMessage({ type: 'fps', fps, frames, count: particles.length });
        frames = 0;
        lastFpsTime = now;
    }
}

self.onmessage = (e) => {
    const msg = e.data;
    switch (msg.type) {
        case 'init':
            canvas = msg.canvas;
            width = msg.width;
            height = msg.height;
            ctx = canvas.getContext('2d');
            palette = msg.palette || palette;
            targetCount = msg.count || 200;
            particles = makeParticles(targetCount);
            t0 = performance.now();
            lastFpsTime = 0;
            frames = 0;
            lastFrameTime = 0;
            requestAnimationFrame(tick);
            self.postMessage({ type: 'ready' });
            break;

        case 'palette':
            palette = msg.palette;
            for (const p of particles) p.c = Math.floor(Math.random() * palette.length);
            break;

        case 'count':
            targetCount = msg.count;
            break;

        case 'pause':
            running = false;
            break;

        case 'resume':
            running = true;
            break;

        case 'transfer_test':
            // Получили ArrayBuffer. Отправляем обратно с transfer —
            // main thread измерит полное round-trip время.
            self.postMessage(
                { type: 'transfer_ack', size: msg.size, t0: msg.t0 },
                [msg.buffer],
            );
            break;
    }
};