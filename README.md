# hello-wasm02

Rust → WebAssembly → JavaScript. Курс от первого `web_sys`-вызова
до OffscreenCanvas, Workers, WASM-в-worker'е, пулов и SAB.

Проект — набор **самостоятельных демо-примеров**: каждая подглава живёт в
`src/app.rs` целиком, заменяется при переключении между темами, инфраструктура
общая и не трогается.

## Требования

- Rust (stable)
- `wasm32-unknown-unknown` target
- `wasm-pack`
- Node.js 20+
- npm
- Vite (устанавливается локально)

## Проверка toolchain

```bash
rustc --version
cargo --version
rustup --version
rustup show
rustup target list --installed
wasm-pack --version
node --version
npm --version
```
````

Установка WASM-target, если ещё нет:

```bash
rustup target add wasm32-unknown-unknown
```

## Основные команды

```bash
# Установка frontend-зависимостей (один раз)
npm install

# Debug-сборка WASM + dev-сервер
npm start

# Только debug-сборка WASM
npm run wasm:dev

# Release-сборка WASM
npm run wasm

# Production-сборка фронтенда (включая WASM release)
npm run build

# Preview production-сборки
npm run preview

# Vite dev-сервер без пересборки WASM
npm run dev
```

Открыть адрес, который покажет Vite (обычно `http://127.0.0.1:5173/`).

## Проверка и тесты

```bash
# Хост-тесты
cargo check
cargo test

# Типы для WASM
cargo check --target wasm32-unknown-unknown

# Браузерные тесты одной подглавы
wasm-pack test --headless --chrome --test wasm_<module>

# Все тесты
npm test
```

## Структура проекта

```text
hello-wasm02/
├── Cargo.toml                    # crate hello-wasm02
├── build.rs
├── src/
│   ├── lib.rs                    # точка входа, is_worker_context()
│   ├── app.rs                    # демо текущей подглавы (заменяется)
│   └── <modules>.rs              # модули подглав
├── tests/
│   ├── wasm_<module>.rs          # браузерные тесты
│   └── <pure>.rs                 # host-тесты
├── web/
│   ├── index.html
│   ├── main.js                   # динамический import("/pkg/hello_wasm02.js")
│   ├── pkg/                      # сборка wasm-pack (out-dir)
│   │   ├── hello_wasm02.js
│   │   ├── hello_wasm02_bg.wasm
│   │   └── ...
│   ├── public/
│   │   ├── sw.js
│   │   └── workers/              # все worker'ы (не обрабатываются Vite)
│   └── styles/
│       └── main.css
├── vite.config.js
├── package.json
├── package-lock.json
└── README.md
```

`wasm-bindgen-cli` — низкоуровневый инструмент постобработки `.wasm`:

```bash
wasm-bindgen --target web --out-dir web/pkg \
    target/wasm32-unknown-unknown/release/hello_wasm02.wasm
```

Обычно вызывается через `wasm-pack build --target web --out-dir web/pkg`.

## Полезные файлы

| Файл                           | Назначение                                                   |
| ------------------------------ | ------------------------------------------------------------ |
| `src/lib.rs`                   | Точка входа, `#[wasm_bindgen(start)]`, `is_worker_context()` |
| `src/app.rs`                   | Демо текущей подглавы                                        |
| `Cargo.toml`                   | Зависимости, web-sys фичи                                    |
| `web/pkg/hello_wasm02.js`      | JS-обёртка (wasm-pack output)                                |
| `web/pkg/hello_wasm02_bg.wasm` | WebAssembly binary                                           |
| `web/main.js`                  | Инициализация WASM в main thread                             |
| `web/public/workers/*.js`      | Worker'ы (module workers)                                    |
| `vite.config.js`               | Dev-сервер, COOP/COEP, proxy                                 |
| `package.json`                 | npm scripts и зависимости                                    |

## Inspection WASM binary

```bash
sudo apt update && sudo apt install wabt
wasm-objdump -x web/pkg/hello_wasm02_bg.wasm
```

---

## Инфраструктура проекта

Раздел фиксирует правила, выработанные в ходе разработки глав 1–22.
Новые демо-примеры создаются **строго по этим правилам**. Инфраструктурные
файлы при этом не трогаются.

### Файлы, которые НЕ меняются при добавлении нового демо

| Файл                  | Роль                                            |
| --------------------- | ----------------------------------------------- |
| `package.json`        | Скрипты сборки. `wasm-pack --out-dir web/pkg`   |
| `vite.config.js`      | Dev-сервер, COOP/COEP, proxy, build             |
| `web/main.js`         | Точка входа: `import("/pkg/hello_wasm02.js")`   |
| `web/index.html`      | Каркас, `<script type="module" src="/main.js">` |
| `web/styles/main.css` | Общие стили (демо инжектируют свои локально)    |
| `web/public/sw.js`    | Service Worker (self-healing в app.rs)          |

### Файлы, которые меняются под каждое новое демо

| Файл                         | Что делать                                     |
| ---------------------------- | ---------------------------------------------- |
| `src/app.rs`                 | Заменить целиком под новую подглаву            |
| `src/<new_module>.rs`        | Создать                                        |
| `src/lib.rs`                 | `+1` строка: `pub mod <new_module>;`           |
| `tests/wasm_<new_module>.rs` | Создать                                        |
| `web/public/workers/<x>.js`  | Создать (если нужен worker)                    |
| `Cargo.toml`                 | `+N` фич web-sys (только если нужны новые API) |

### Сборка и URL WASM-модуля

WASM собирается в `web/pkg/`, доступен по URL `/pkg/hello_wasm02.js`.

```
web/pkg/hello_wasm02.js         ← ESM-обёртка (wasm-pack output)
web/pkg/hello_wasm02_bg.wasm    ← бинарник
```

Связка работает благодаря:

- `package.json` → `wasm-pack build --out-dir web/pkg`
- `vite.config.js` → `root: 'web'`

С `root: 'web'` URL `/pkg/hello_wasm02.js` резолвится в
`web/pkg/hello_wasm02.js`. Vite обрабатывает его **как source-модуль**:
`new URL('hello_wasm02_bg.wasm', import.meta.url)` внутри работает корректно
и в dev, и в build.

**Почему не `public/pkg/`.** Vite 8 запрещает импортировать JS/CSS из
`public/` как модуль. Только через `<script src>` — не подходит для ESM-обёртки
wasm-pack и для module workers.

**Почему не alias `@pkg`.** Двойной путь к одному файлу (alias + `/pkg/`)
приводит к двум разным экземплярам WASM в одном контексте и к рассинхронизации
кэшей. Один URL — единственный источник.

### Worker'ы

Все worker'ы лежат в `web/public/workers/<name>.js`. Vite их не обрабатывает —
отдаёт браузеру как есть.

Шаблон:

```javascript
// <глава>.<номер>. <краткое описание>.

self.postMessage({ type: 'boot', stage: 'worker_started' });

let ready = false;
let wasm = null;
const pending = [];

self.onmessage = (e) => {
  if (!ready) {
    pending.push(e);
    return;
  }
  handle(e.data);
};

self.addEventListener('error', (ev) => {
  self.postMessage({ type: 'runtime_error', message: ev.message || '?' });
});

(async () => {
  try {
    const mod = await import('/pkg/hello_wasm02.js');
    await mod.default();
    wasm = mod;
    ready = true;
    self.postMessage({ type: 'ready', instance: mod.instance_id?.() });
    while (pending.length) handle(pending.shift().data);
  } catch (err) {
    self.postMessage({
      type: 'init_error',
      message: String(err) + (err?.stack ? ' | ' + err.stack : ''),
    });
  }
})();

function handle(msg) {
  // Обработка команд от main thread
}
```

Требования:

1. **Динамический `import()`**, не статический. Статический валит весь
   модуль при любой ошибке — ни `boot`, ни `onmessage` не устанавливаются.
2. **`boot` до импорта.** Если main thread видит `boot` — worker жив.
3. **`onmessage` — синхронно** до `await import()`. Сообщения копятся
   в `pending`, чтобы не терять их при медленном init.
4. **`onerror` + `unhandledrejection`** — любая ошибка уходит в main thread.
5. **`init_error` в catch** с полным стеком.

Создание worker'а — только **module worker**:

```rust
let opts = web_sys::WorkerOptions::new();
opts.set_type(web_sys::WorkerType::Module);
let worker = web_sys::Worker::new_with_options("/workers/foo.js", &opts)?;
```

### `lib.rs` — worker-safe точка входа

`#[wasm_bindgen(start)]` исполняется в **каждом** контексте инстанциации WASM.
В worker'е нет `window`. Точка входа обязана это учитывать.

```rust
#[wasm_bindgen(start)]
pub fn run() -> Result<(), JsValue> {
    if is_worker_context() {
        return Ok(());
    }
    logging::init();
    app::run()
}

/// true, если код исполняется в Worker-контексте (нет `window`).
fn is_worker_context() -> bool {
    let global = js_sys::global();
    match js_sys::Reflect::get(&global, &JsValue::from_str("window")) {
        Ok(v) => v.is_undefined(),
        Err(_) => true,
    }
}
```

`app::run()` — **только демо-логика**. Никаких проверок контекста,
никаких `logging::init()`. Всё это в `lib.rs`.

### Создание нового демо — шаги

1. **Rust-модуль** — `src/<new_module>.rs` с `#[wasm_bindgen]` функциями,
   feature detection, утилитами `format_str` / `bool_of`.
2. **Регистрация** — `pub mod <new_module>;` в `src/lib.rs`.
3. **Тесты** — `tests/wasm_<new_module>.rs`.
4. **Worker** (если нужен) — `web/public/workers/<x>.js` по шаблону.
5. **Демо** — заменить `src/app.rs` целиком.
6. **Запуск** — `npm start`.

### Стили и CSS-префиксы

Каждая подглава использует **свой префикс** CSS-классов, уникальный
в проекте. Примеры: `ow-` (глава 22), `oc-` (22.1), `dw-` (22.2),
`tr-` (22.3), `ww-` (22.4), `wp-` (22.5), `fr-` (22.6).

Стили инжектируются в документ через `<style>` в `inject_styles()`,
правок в `main.css` не требуется.

### Заголовки COOP/COEP

Включены **безусловно** в `vite.config.js`, применяются и к `server`
(dev), и к `preview`:

```js
const coiHeaders = {
  'Cross-Origin-Opener-Policy': 'same-origin',
  'Cross-Origin-Embedder-Policy': 'require-corp',
};
```

Не мешают демо без `SharedArrayBuffer`. Обязательны для SAB и
`crossOriginIsolated`.

Feature detection на стороне Rust:

```rust
pub fn is_shared_array_buffer_supported() -> bool {
    let global = js_sys::global();
    matches!(
        js_sys::Reflect::get(&global, &JsValue::from_str("SharedArrayBuffer")),
        Ok(v) if v.is_function()
    )
}
```

### Работа с событиями и ошибками

Не читать `ErrorEvent::message()` напрямую. Строгий биндинг падает на
базовом `Event` без полей `ErrorEvent`:

```
wasm-bindgen: imported JS function that was not marked as `catch`
threw an error: expected a string argument, found undefined
```

Безопасное чтение — через `web_sys::Event` + `Reflect::get` с fallback:

```rust
let closure = wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::Event)>::new(
    move |ev: web_sys::Event| {
        let target: &JsValue = ev.as_ref();
        let msg = js_sys::Reflect::get(target, &JsValue::from_str("message"))
            .ok()
            .and_then(|v| v.as_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| String::from("worker error (no message)"));
        // ...
    },
);
```

Ошибки хранятся в `state.error: Option<String>`, показываются в
`update_stats`. Не перетираются rAF-счётчиком. Сбрасываются при `ready`.

### Service Worker

Демо проверяет наличие активного Service Worker'а и снимает регистрацию
один раз через self-healing:

```rust
if has_sw_controller() {
    show_sw_notice(&document)?;
    unregister_sw_and_reload(&document)?;
    return Ok(());
}
```

Защита от цикла — `sessionStorage`-флаг с уникальным ключом
(`<prefix>-sw-cleared-<chapter>`).

Чужой Service Worker перехватывает `import "/pkg/hello_wasm02.js"` и
ломает загрузку. Демо обязано снять регистрацию, если она есть.

### Правила оформления `app.rs`

- Содержит **только текущую главу**. Предыдущие не накапливаются.
- `format_str(format_args!(...))` в tail-позициях — обход бага rust-analyzer.
- `bool_of(matches!(...))` в tail-позициях — то же.
- RefMut-дисциплина: узкие scope, `borrow` после `drop`.
- Все замыкания через `Closure::new` — `document.clone()`, `state.clone()`.
  Каждое замыкание получает свой клон `Rc<RefCell<State>>` и `Document`.
- `Rc<RefCell<State>>` для общего состояния между обработчиками.
- Feature detection перед использованием API.
- Никаких неиспользуемых импортов.
- Явный `return expr;` в функциях, где r-a теряет тип tail-выражения.

### Чек-лист нового демо

- [ ] `src/<new_module>.rs` создан, экспортирует `#[wasm_bindgen]` функции
- [ ] `src/lib.rs` содержит `pub mod <new_module>;`
- [ ] `src/lib.rs::run()` содержит `is_worker_context()` проверку
- [ ] `tests/wasm_<new_module>.rs` создан, все тесты проходят
- [ ] `src/app.rs` содержит только текущую главу, свой CSS-префикс
- [ ] Worker (если нужен) создан по шаблону
- [ ] Динамический `import("/pkg/hello_wasm02.js")` в worker'е
- [ ] `onmessage` — синхронно, до import
- [ ] `init_error` с полным стеком в catch
- [ ] `ErrorEvent::message()` не вызывается напрямую
- [ ] Ошибки хранятся в `state.error`, не перетираются
- [ ] Service Worker self-healing присутствует
- [ ] `package.json`, `vite.config.js`, `main.js`, `index.html` **не изменялись**
- [ ] `cargo check --target wasm32-unknown-unknown` без предупреждений
- [ ] `wasm-pack test --headless --chrome --test wasm_<new_module>` — все зелёные
