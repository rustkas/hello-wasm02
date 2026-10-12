//! 21.1. HtmlCanvasElement: размеры и HiDPI.
//!
//! Ключевая идея: **размер буфера canvas ≠ размер элемента на странице**.
//! На Retina-экранах devicePixelRatio = 2 (и больше), и без умножения
//! буфер растягивается браузером — картинка размывается.
//!
//! Правильно: `canvas.width = css_w * dpr`, `style.width = css_w + "px"`,
//! `ctx.scale(dpr, dpr)`. Тогда 1 CSS-пиксель = `dpr × dpr` физических.
//!
//! ## Про `style()` и `dyn_into`
//!
//! `HtmlCanvasElement::style()` не существует — метод объявлен на
//! `HtmlElement`, а Rust не делает авто-upcast между типами web-sys.
//! Используем `unchecked_ref::<HtmlElement>()` — это **безопасный**
//! upcast по цепочке наследования DOM (`HtmlCanvasElement extends
//! HtmlElement`).
//!
//! `JsCast::dyn_into::<T>()` возвращает `Result<T, Self>` — то есть для
//! `Object` ошибка имеет тип `Object`, а не `JsValue`. Чтобы вернуть
//! `Result<_, JsValue>`, добавляем `.map_err(|_| JsValue::from_str(...))`.

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, HtmlElement};

// =============================================================================
// Чистые функции
// =============================================================================

/// Ограничивает `devicePixelRatio` разумным диапазоном `[1.0, 4.0]`.
///
/// Обработка особых случаев:
/// - `NaN` → `1.0` (безопасное значение);
/// - `+∞` → `4.0` (верхняя граница — «очень большое число»);
/// - `-∞`, `0`, отрицательные → `1.0` (нижняя граница);
/// - конечные числа → `clamp(1.0, 4.0)`.
///
/// `f64::clamp` сам по себе не подходит: на `NaN` он возвращает `NaN`,
/// а не безопасный default. Поэтому `NaN` обрабатывается явно.
pub fn clamp_dpr(dpr: f64) -> f64 {
    if dpr.is_nan() {
        return 1.0;
    }
    dpr.clamp(1.0, 4.0)
}

/// Физический размер буфера для заданных CSS-размеров и DPR.
/// Округление к ближайшему, минимум 1 пиксель в каждой стороне.
pub fn compute_canvas_size(css_w: f64, css_h: f64, dpr: f64) -> (u32, u32) {
    let dpr = clamp_dpr(dpr);
    let w = (css_w.max(0.0) * dpr).round().max(1.0);
    let h = (css_h.max(0.0) * dpr).round().max(1.0);
    (w as u32, h as u32)
}

// =============================================================================
// Web-обвязка
// =============================================================================

/// Текущий devicePixelRatio, либо 1.0, если окна нет.
pub fn get_device_pixel_ratio() -> f64 {
    web_sys::window()
        .map(|w| w.device_pixel_ratio())
        .unwrap_or(1.0)
}

/// Достаёт 2d-контекст как `CanvasRenderingContext2d`.
///
/// Помощник, который инкапсулирует пару преобразований:
/// `getContext("2d")` даёт `Option<Object>`, а `dyn_into` возвращает
/// `Result<_, Object>` — нужен явный `.map_err`, чтобы получить
/// ожидаемый `Result<_, JsValue>`.
fn get_2d_context(
    canvas: &HtmlCanvasElement,
) -> Result<CanvasRenderingContext2d, JsValue> {
    canvas
        .get_context("2d")?
        .ok_or_else(|| JsValue::from_str("no 2d context"))?
        .dyn_into::<CanvasRenderingContext2d>()
        .map_err(|_| JsValue::from_str("context is not CanvasRenderingContext2d"))
}

/// Наивная настройка: буфер = CSS-размер, без DPR.
/// На Retina-экране даст размытую картинку.
pub fn setup_canvas_naive(
    canvas: &HtmlCanvasElement,
    css_w: f64,
    css_h: f64,
) -> Result<CanvasRenderingContext2d, JsValue> {
    canvas.set_width(css_w.max(1.0) as u32);
    canvas.set_height(css_h.max(1.0) as u32);

    // style() живёт на HtmlElement — upcast через unchecked_ref.
    let style = canvas.unchecked_ref::<HtmlElement>().style();
    let w_str = format!("{css_w}px");
    let h_str = format!("{css_h}px");
    style.set_property("width", &w_str)?;
    style.set_property("height", &h_str)?;

    get_2d_context(canvas)
}

/// Правильная настройка: буфер × DPR, рисование в CSS-координатах.
pub fn setup_canvas_hidpi(
    canvas: &HtmlCanvasElement,
    css_w: f64,
    css_h: f64,
    dpr: f64,
) -> Result<CanvasRenderingContext2d, JsValue> {
    let dpr = clamp_dpr(dpr);
    let (buf_w, buf_h) = compute_canvas_size(css_w, css_h, dpr);

    canvas.set_width(buf_w);
    canvas.set_height(buf_h);

    let style = canvas.unchecked_ref::<HtmlElement>().style();
    let w_str = format!("{css_w}px");
    let h_str = format!("{css_h}px");
    style.set_property("width", &w_str)?;
    style.set_property("height", &h_str)?;

    let ctx = get_2d_context(canvas)?;
    ctx.scale(dpr, dpr)?;
    Ok(ctx)
}

/// Тестовый паттерн в CSS-координатах: чувствителен к weble.
pub fn draw_test_pattern(
    ctx: &CanvasRenderingContext2d,
    css_w: f64,
    css_h: f64,
) -> Result<(), JsValue> {
    // Фон
    ctx.set_fill_style_str("#0d1117");
    ctx.fill_rect(0.0, 0.0, css_w, css_h);

    // Сетка 20×20 CSS-пикселей — тонкие линии.
    ctx.set_stroke_style_str("#21262d");
    ctx.set_line_width(0.5);
    let mut x = 0.0;
    while x < css_w {
        ctx.begin_path();
        ctx.move_to(x, 0.0);
        ctx.line_to(x, css_h);
        ctx.stroke();
        x += 20.0;
    }
    let mut y = 0.0;
    while y < css_h {
        ctx.begin_path();
        ctx.move_to(0.0, y);
        ctx.line_to(css_w, y);
        ctx.stroke();
        y += 20.0;
    }

    // Диагональ — самый чувствительный к мылу элемент.
    ctx.set_stroke_style_str("#58a6ff");
    ctx.set_line_width(1.0);
    ctx.begin_path();
    ctx.move_to(20.0, 20.0);
    ctx.line_to(css_w - 20.0, css_h - 20.0);
    ctx.stroke();

    // Тонкие окружности.
    ctx.set_stroke_style_str("#3fb950");
    ctx.set_line_width(1.0);
    for r in [15.0_f64, 25.0, 35.0] {
        ctx.begin_path();
        ctx.arc(css_w * 0.5, css_h * 0.5, r, 0.0, std::f64::consts::TAU)?;
        ctx.stroke();
    }

    // Мелкий текст.
    ctx.set_fill_style_str("#c9d1d9");
    ctx.set_font("11px monospace");
    ctx.fill_text("HiDPI check: 1 CSS px", 24.0, 36.0)?;
    ctx.set_font("8px monospace");
    ctx.fill_text("tiny text 8px", 24.0, 52.0)?;

    Ok(())
}