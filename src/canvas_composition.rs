//! 21.3. Состояние, трансформации, композиция.
//!
//! Три группы операций над контекстом:
//!
//! 1. **Стек состояний** — `save()` / `restore()`. Запоминает и восстанавливает
//!    всё, что входит в состояние: стили, матрицу трансформации, clip,
//!    `globalAlpha`, `globalCompositeOperation`, `font`, `textAlign` и др.
//!
//! 2. **Матрица трансформации** — `translate`, `rotate`, `scale`,
//!    `setTransform`, `resetTransform`. Трансформации применяются к
//!    системе координат, а не к уже нарисованному.
//!
//! 3. **Композиция** — `globalAlpha` (прозрачность) и
//!    `globalCompositeOperation` (режим смешивания). Плюс `clip()` —
//!    маска, ограничивающая область рисования.
//!
//! Модуль не зависит от предыдущих подглав: только `web_sys` и стандартная
//! библиотека.

use wasm_bindgen::prelude::*;
use web_sys::{CanvasRenderingContext2d, DomMatrix};

// =============================================================================
// Композиция слоёв
// =============================================================================

/// Описание прямоугольного слоя.
#[derive(Clone, Copy, Debug)]
pub struct Layer {
    pub color: &'static str,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Рисует три наложенных прямоугольника. Верхний слой — красный —
/// рисуется с заданными `alpha` и `blend_mode`; при `use_clip = true`
/// его область ограничена кругом в правой части холста.
///
/// Слои нарисованы так, чтобы пересечения были видны глазами:
/// - синий и зелёный перекрываются слегка;
/// - красный перекрывает оба.
pub fn draw_layers(
    ctx: &CanvasRenderingContext2d,
    w: f64,
    h: f64,
    top_alpha: f64,
    blend_mode: &str,
    use_clip: bool,
) -> Result<(), JsValue> {
    // Фон.
    ctx.set_global_alpha(1.0);
    ctx.set_global_composite_operation("source-over")?;
    ctx.set_fill_style_str("#0d1117");
    ctx.fill_rect(0.0, 0.0, w, h);

    // Слой 1 — синий.
    let l1 = Layer {
        color: "#1f6feb",
        x: 30.0,
        y: 30.0,
        w: 140.0,
        h: 140.0,
    };
    ctx.set_fill_style_str(l1.color);
    ctx.fill_rect(l1.x, l1.y, l1.w, l1.h);

    // Слой 2 — зелёный.
    let l2 = Layer {
        color: "#3fb950",
        x: 120.0,
        y: 80.0,
        w: 140.0,
        h: 140.0,
    };
    ctx.set_fill_style_str(l2.color);
    ctx.fill_rect(l2.x, l2.y, l2.w, l2.h);

    // Слой 3 — красный с alpha/blend/clip.
    if use_clip {
        ctx.save();
        // Круговой clip: центр вокруг правой части холста.
        ctx.begin_path();
        let cx = w * 0.72;
        let cy = h * 0.5;
        let r = h * 0.42;
        ctx.arc(cx, cy, r, 0.0, std::f64::consts::TAU)?;
        ctx.clip();
    }

    ctx.set_global_alpha(top_alpha.clamp(0.0, 1.0));
    ctx.set_global_composite_operation(blend_mode)?;
    let l3 = Layer {
        color: "#f85149",
        x: 210.0,
        y: 30.0,
        w: 140.0,
        h: 140.0,
    };
    ctx.set_fill_style_str(l3.color);
    ctx.fill_rect(l3.x, l3.y, l3.w, l3.h);

    if use_clip {
        ctx.restore();
    }

    // Восстановление чистого состояния для последующих операций.
    ctx.set_global_alpha(1.0);
    ctx.set_global_composite_operation("source-over")?;
    Ok(())
}

// =============================================================================
// Стек состояний: save / restore
// =============================================================================

/// Рисует три квадрата. Второй смещён через `translate(100, 40)`.
/// Третий рисуется **после** второго, но с `restore()` или без —
/// в зависимости от флага `with_restore`.
///
/// `with_restore = true`:  третий квадрат снова в исходной точке (20, 20).
/// `with_restore = false`: трансформация накопилась, третий квадрат
///                         смещён на (120, 60).
pub fn draw_transform_demo(
    ctx: &CanvasRenderingContext2d,
    w: f64,
    h: f64,
    with_restore: bool,
) -> Result<(), JsValue> {
    ctx.set_transform(1.0, 0.0, 0.0, 1.0, 0.0, 0.0)?;
    ctx.set_global_alpha(1.0);
    ctx.set_global_composite_operation("source-over")?;

    ctx.set_fill_style_str("#0d1117");
    ctx.fill_rect(0.0, 0.0, w, h);

    // Квадрат 1 — без трансформации.
    ctx.set_fill_style_str("#58a6ff");
    ctx.fill_rect(20.0, 20.0, 60.0, 60.0);

        // Квадрат 2 — со сдвигом.
    if with_restore {
        ctx.save();
    }
    ctx.translate(100.0, 40.0)?;
    ctx.set_fill_style_str("#3fb950");
    ctx.fill_rect(20.0, 20.0, 60.0, 60.0);
    if with_restore {
        ctx.restore();
    }

    // Квадрат 3 — после сдвига.
    ctx.set_fill_style_str("#f0883e");
    ctx.fill_rect(20.0, 20.0, 60.0, 60.0);

    // Гарантируем чистое состояние на выходе.
    ctx.set_transform(1.0, 0.0, 0.0, 1.0, 0.0, 0.0)?;
    Ok(())
}

// =============================================================================
// Clipping
// =============================================================================

/// Заполняет весь холст чёрным, затем ограничивает рисование кругом
/// и заливает белым. Результат: круг белый, остальное — чёрное.
///
/// Тесты читают пиксели в центре (должно быть белым) и в углу
/// (должно остаться чёрным) — так проверяется, что clip действительно
/// ограничивает область.
pub fn draw_clip_demo(
    ctx: &CanvasRenderingContext2d,
    w: f64,
    h: f64,
) -> Result<(), JsValue> {
    ctx.set_transform(1.0, 0.0, 0.0, 1.0, 0.0, 0.0)?;
    ctx.set_global_alpha(1.0);
    ctx.set_global_composite_operation("source-over")?;

    // Чёрный фон.
    ctx.set_fill_style_str("#000000");
    ctx.fill_rect(0.0, 0.0, w, h);

    // Клип: круг в центре.
    ctx.save();
    ctx.begin_path();
    ctx.arc(w * 0.5, h * 0.5, h * 0.4, 0.0, std::f64::consts::TAU)?;
    ctx.clip();

    // Белая заливка — попадёт только в круг.
    ctx.set_fill_style_str("#ffffff");
    ctx.fill_rect(0.0, 0.0, w, h);
    ctx.restore();

    Ok(())
}

// =============================================================================
// Чтение матрицы трансформации
// =============================================================================

/// Возвращает `(a, b, c, d, e, f)` — компоненты текущей матрицы 2D-аффинного
/// преобразования. Матрица хранится как `[a c e; b d f]`.
///
/// Используется в тестах для проверки, что `save()` / `restore()`
/// действительно восстанавливают трансформацию.
pub fn read_transform(
    ctx: &CanvasRenderingContext2d,
) -> (f64, f64, f64, f64, f64, f64) {
    match ctx.get_transform() {
        Ok(m) => matrix_components(&m),
        Err(_) => (1.0, 0.0, 0.0, 1.0, 0.0, 0.0),
    }
}

fn matrix_components(m: &DomMatrix) -> (f64, f64, f64, f64, f64, f64) {
    (m.a(), m.b(), m.c(), m.d(), m.e(), m.f())
}