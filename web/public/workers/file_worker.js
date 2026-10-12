// Web Worker для файловых операций.
//
// Обрабатывает три типа сообщений:
// 1. { type: "read",       id, file }               — первые 64 байта
// 2. { type: "hash",       id, file }               — SHA-256 через crypto.subtle
// 3. { type: "hash_chunk", id, file, start, end }   — SHA-256 куска файла
//
// Ответы:
//   { type: "result",            id, name, size, headHex, headLen }
//   { type: "hash_result",       id, name, size, hex }
//   { type: "hash_chunk_result", id, start, end, bytes, hex }
//   { type: "error",             id, message }

const HEAD_SIZE = 64;

self.onmessage = (event) => {
    const data = event.data;
    if (!data) return;

    if (data.type === "read") {
        handleRead(data);
    } else if (data.type === "hash") {
        handleHash(data);
    } else if (data.type === "hash_chunk") {
        handleHashChunk(data);
    } else {
        self.postMessage({ type: "error", message: "unknown message type" });
    }
};

function handleRead(data) {
    const file = data.file;
    const id = data.id;
    if (!file) {
        self.postMessage({ type: "error", id, message: "no file" });
        return;
    }
    try {
        const reader = new FileReaderSync();
        const head = file.slice(0, HEAD_SIZE);
        const buffer = reader.readAsArrayBuffer(head);
        const uint8 = new Uint8Array(buffer);
        let hex = "";
        for (let i = 0; i < uint8.length; i++) {
            if (i > 0) hex += " ";
            hex += uint8[i].toString(16).padStart(2, "0").toUpperCase();
        }
        self.postMessage({
            type: "result",
            id,
            name: file.name,
            size: file.size,
            headHex: hex,
            headLen: uint8.length,
        });
    } catch (err) {
        self.postMessage({ type: "error", id, message: String(err) });
    }
}

async function handleHash(data) {
    const file = data.file;
    const id = data.id;
    if (!file) {
        self.postMessage({ type: "error", id, message: "no file" });
        return;
    }
    try {
        const arrayBuffer = await file.arrayBuffer();
        const hashBuffer = await crypto.subtle.digest("SHA-256", arrayBuffer);
        const hex = bytesToHex(new Uint8Array(hashBuffer));
        self.postMessage({
            type: "hash_result",
            id,
            name: file.name,
            size: file.size,
            hex,
        });
    } catch (err) {
        self.postMessage({ type: "error", id, message: String(err) });
    }
}

async function handleHashChunk(data) {
    const file = data.file;
    const start = data.start;
    const end = data.end;
    const id = data.id;
    if (!file) {
        self.postMessage({ type: "error", id, message: "no file" });
        return;
    }
    try {
        const slice = file.slice(start, end);
        const buffer = await slice.arrayBuffer();
        const hashBuffer = await crypto.subtle.digest("SHA-256", buffer);
        const hex = bytesToHex(new Uint8Array(hashBuffer));
        self.postMessage({
            type: "hash_chunk_result",
            id,
            start,
            end,
            bytes: buffer.byteLength,
            hex,
        });
    } catch (err) {
        self.postMessage({ type: "error", id, message: String(err) });
    }
}

function bytesToHex(uint8) {
    let hex = "";
    for (let i = 0; i < uint8.length; i++) {
        hex += uint8[i].toString(16).padStart(2, "0");
    }
    return hex;
}