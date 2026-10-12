//! 21.2. Drawing API: фигуры, пути, текст.
//!
//! Всё рисование идёт через `CanvasRenderingContext2d`. Прямоугольники —
//! четыре простых метода (`fillRect`, `strokeRect`, `clearRect`, `rect`).
//! Всё остальное — **пути**: `beginPath` → набор сегментов (`lineTo`,
//! `arc`, `bezierCurveTo`, `quadraticCurveTo`) → `fill()` или `stroke()`.
//!
//! Стиль — **состояние контекста**: `fillStyle`, `strokeStyle`, `lineWidth`,
//! `lineCap`, `lineJoin`, `font`. Устанавливается один раз, действует на
//! все последующие операции, пока не изменится.
//!
//! Модуль держит `Brush` — компактное представление этого состояния —
//! и одну функцию `draw_scene`, которая рисует демонстрационный набор фигур.

use wasm_bindgen::prelude::*;
use web_sys::{CanvasGradient, CanvasRenderingContext2d, TextMetrics};

// =============================================================================
// Кисть — состояние контекста
// =============================================================================

/// Параметры кисти. Все поля — `&'static str`, потому что в демо это
/// фиксированные палитры из константных массивов.
#[derive(Clone, Copy, Debug)]
pub struct Brush {
    pub stroke: &'static str,
    pub fill: &'static str,
    pub line_width: f64,
    pub line_cap: &'static str,
    pub line_join: &'static str,
}

impl Default for Brush {
    fn default() -> Self {
        Self {
            stroke: "#58a6ff",
            fill: "rgba(88, 166, 255, 0.25)",
            line_width: 2.0,
            line_cap: "round",
            line_join: "round",
        }
    }
}

// =============================================================================
// Рисование
// =============================================================================

/// Очищает холст фоновым цветом.
pub fn clear(ctx: &CanvasRenderingContext2d, w: f64, h: f64, bg: &str) {
    ctx.set_fill_style_str(bg);
    ctx.fill_rect(0.0, 0.0, w, h);
}

/// Устанавливает стиль кисти на контекст.
fn apply_brush(ctx: &CanvasRenderingContext2d, brush: Brush) {
    ctx.set_line_width(brush.line_width);
    ctx.set_line_cap(brush.line_cap);
    ctx.set_line_join(brush.line_join);
    ctx.set_stroke_style_str(brush.stroke);
    ctx.set_fill_style_str(brush.fill);
}

/// Создаёт линейный градиент между двумя точками с двумя стопами.
/// Возвращает `None`, если браузер по какой-то причине не дал объект.
fn make_gradient(
    ctx: &CanvasRenderingContext2d,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    c0: &str,
    c1: &str,
) -> Option<CanvasGradient> {
    let g = ctx.create_linear_gradient(x0, y0, x1, y1);
    g.add_color_stop(0.0, c0).ok()?;
    g.add_color_stop(1.0, c1).ok()?;
    Some(g)
}

/// Рисует демонстрационный набор фигур в **CSS-координатах** `(0..w, 0..h)`.
///
/// Состав сцены:
/// 1. прямоугольник — заливка + обводка (`fillRect` + `strokeRect`);
/// 2. окружность — путь `arc` + `fill` + `stroke`;
/// 3. ломаная — путь `moveTo` + четыре `lineTo`, только `stroke`;
/// 4. кубическая кривая Безье — `bezierCurveTo`, только `stroke`;
/// 5. квадратичная кривая — `quadraticCurveTo`, только `stroke`;
/// 6. три размера текста — 16px, 12px, 9px (`fillText` + `font`);
/// 7. прямоугольник с линейным градиентом (`createLinearGradient`).
pub fn draw_scene(
    ctx: &CanvasRenderingContext2d,
    w: f64,
    h: f64,
    brush: Brush,
) -> Result<(), JsValue> {
    clear(ctx, w, h, "#0d1117");
    apply_brush(ctx, brush);

    // 1. Прямоугольник — заливка и обводка.
    ctx.fill_rect(20.0, 20.0, 100.0, 70.0);
    ctx.stroke_rect(20.0, 20.0, 100.0, 70.0);

    // 2. Окружность — путь + fill + stroke.
    ctx.begin_path();
    ctx.arc(200.0, 55.0, 35.0, 0.0, std::f64::consts::TAU)?;
    ctx.fill();
    ctx.stroke();

    // 3. Ломаная — пять точек, только обводка.
    ctx.begin_path();
    ctx.move_to(260.0, 90.0);
    ctx.line_to(290.0, 30.0);
    ctx.line_to(320.0, 80.0);
    ctx.line_to(350.0, 25.0);
    ctx.line_to(380.0, 70.0);
    ctx.stroke();

    // 4. Кубическая кривая Безье — «волна».
    ctx.begin_path();
    ctx.move_to(20.0, 140.0);
    ctx.bezier_curve_to(90.0, 100.0, 170.0, 180.0, 240.0, 140.0);
    ctx.stroke();

    // 5. Квадратичная кривая — «горка».
    ctx.begin_path();
    ctx.move_to(260.0, 140.0);
    ctx.quadratic_curve_to(320.0, 90.0, 380.0, 140.0);
    ctx.stroke();

    // 6. Текст — три размера, для наглядности разницы.
    ctx.set_fill_style_str("#c9d1d9");
    ctx.set_font("16px monospace");
    ctx.fill_text("Large 16px", 20.0, 180.0)?;
    ctx.set_font("12px monospace");
    ctx.fill_text("Medium 12px", 20.0, 200.0)?;
    ctx.set_font("9px monospace");
    ctx.fill_text("Small 9px", 20.0, 216.0)?;

    // 7. Градиент — линейный переход между цветом кисти и зелёным.
    if let Some(grad) = make_gradient(
        ctx, 220.0, 160.0, 380.0, 210.0, brush.stroke, "#3fb950",
    ) {
        ctx.set_fill_style_canvas_gradient(&grad);
        ctx.fill_rect(220.0, 160.0, 160.0, 50.0);
    }

    Ok(())
}

/// Возвращает `(width, ascent, descent)` для строки при текущем `font`.
/// Использует `measureText`, нужен для центрирования и подбора размера.
pub fn measure(
    ctx: &CanvasRenderingContext2d,
    text: &str,
) -> Result<(f64, f64, f64), JsValue> {
    let m: TextMetrics = ctx.measure_text(text)?;
    Ok((
        m.width(),
        m.actual_bounding_box_ascent(),
        m.actual_bounding_box_descent(),
    ))
}