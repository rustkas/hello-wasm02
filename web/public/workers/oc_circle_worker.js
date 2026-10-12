// 22.1. Circle worker.
//
// Worker владеет OffscreenCanvas, переданным через
// transferControlToOffscreen. Рисует ту же анимацию пульсирующих
// кругов, что и main thread, и отправляет FPS в main thread.
//
// Никаких команд, кроме init: подглава про сам OffscreenCanvas,
// а не про обмен сообщениями.

let ctx = null;
let width = 0;
let height = 0;
let frames = 0;
let lastFpsAt = 0;

function draw(now) {
    requestAnimationFrame(draw);
    if (!ctx) return;

    ctx.fillStyle = "#0d1117";
    ctx.fillRect(0, 0, width, height);
    ctx.lineWidth = 4;

    const cx = width / 2;
    const cy = height / 2;
    const t = now / 1000;

    for (let i = 0; i < 5; i++) {
        const offset = i * 0.5;
        const r = 20 + Math.abs(Math.sin(t + offset)) * 60;
        const angle = t * 0.7 + offset;
        const px = cx + Math.cos(angle) * 70;
        const py = cy + Math.sin(angle) * 70;
        const hue = (i * 60 + (t * 40) % 360) % 360;

        ctx.beginPath();
        ctx.arc(px, py, r, 0, Math.PI * 2);
        ctx.strokeStyle = `hsl(${hue.toFixed(0)}, 70%, 60%)`;
        ctx.stroke();
    }

    frames++;
    if (!lastFpsAt) lastFpsAt = now;
    if (now - lastFpsAt >= 500) {
        const fps = (frames * 1000) / (now - lastFpsAt);
        self.postMessage({ type: "fps", fps });
        frames = 0;
        lastFpsAt = now;
    }
}

self.onmessage = (e) => {
    const msg = e.data;
    if (msg.type === "init") {
        width = msg.width;
        height = msg.height;
        ctx = msg.canvas.getContext("2d");
        requestAnimationFrame(draw);
    }
};