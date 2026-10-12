#!/usr/bin/env python3
"""Локальный сервер для WASM-демо.

## Зачем не `python3 -m http.server`

`http.server` в некоторых версиях Python отдаёт `.js` как `text/plain`.
Модульные воркеры (`new Worker(url, {type:'module'})`) отказываются
загружать модуль с неправильным MIME-типом — и делают это **молча**,
без события `error` на объекте Worker. Именно поэтому демо с паникой
застревает в состоянии `spawning…`.

## Использование

    cd ~/projects/wa/hello-wasm02
    python3 serve.py
    # Откройте http://127.0.0.1:5173/

## Альтернатива

    npx serve .
"""

from __future__ import annotations

import http.server
import socketserver
import sys
from typing import ClassVar

PORT = 5173


class WasmHandler(http.server.SimpleHTTPRequestHandler):
    """Обработчик HTTP с правильными MIME-типами для WASM-демо.

    Наследует `SimpleHTTPRequestHandler`, переопределяя карту расширений
    и добавляя заголовки против кэширования. Без этого модульные
    воркеры не загружаются из-за `text/plain` для `.js`.
    """

    # Правильные MIME-типы для WASM-демо.
    # Основные правки: .js → text/javascript, .wasm → application/wasm.
    # ClassVar — чтобы линтеры не считали это мутабельным атрибутом
    # экземпляра (RUF012).
    extensions_map: ClassVar[dict[str, str]] = {
        **http.server.SimpleHTTPRequestHandler.extensions_map,
        ".js": "text/javascript",
        ".mjs": "text/javascript",
        ".wasm": "application/wasm",
        ".json": "application/json",
        ".html": "text/html",
    }

    def end_headers(self) -> None:
        """Добавляет заголовки против кэша и вызывает родителя.

        Отключаем кэш — иначе после `wasm-pack build` браузер может
        отдавать старый `.wasm`.
        """
        self.send_header("Cache-Control", "no-store, no-cache, must-revalidate")
        self.send_header("Pragma", "no-cache")
        super().end_headers()


def main() -> None:
    """Запускает сервер на `PORT`. Завершается с кодом 1, если порт занят."""
    try:
        with socketserver.TCPServer(("", PORT), WasmHandler) as httpd:
            print(f"serving at http://127.0.0.1:{PORT}/")
            print("Ctrl+C to stop")
            httpd.serve_forever()
    except OSError as e:
        print(f"cannot bind port {PORT}: {e}", file=sys.stderr)
        sys.exit(1)
    except KeyboardInterrupt:
        print("\nstopped")


if __name__ == "__main__":
    main()
    