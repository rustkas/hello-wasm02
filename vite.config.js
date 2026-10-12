import { defineConfig } from 'vite';

const coiHeaders = {
    'Cross-Origin-Opener-Policy': 'same-origin',
    'Cross-Origin-Embedder-Policy': 'require-corp',
};

export default defineConfig({
    root: 'web',
    server: {
        host: '127.0.0.1',
        port: 5173,
        fs: { allow: ['..'] },
        headers: coiHeaders,
        proxy: {
            '/api': { target: 'http://127.0.0.1:3000', changeOrigin: true },
        },
    },
    preview: {
        host: '127.0.0.1',
        port: 5173,
        headers: coiHeaders,
    },
    build: {
        outDir: '../dist',
        emptyOutDir: true,
    },
});