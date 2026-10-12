//! 21.5. Загрузка изображений: `<img>`, `ImageBitmap`, EXIF.
//!
//! Три способа получить пиксели из файла:
//! - `<img>` + `decode()` — классика, работает везде;
//! - `createImageBitmap(blob)` — прямая декодировка без DOM;
//! - `OffscreenCanvas` — для работы с уже декодированным изображением.
//!
//! ## EXIF-ориентация
//!
//! Фото с телефона приходит с метаданными «повернуть на 90°».
//! Ни `<img>`, ни `createImageBitmap` без явной опции не применяют
//! их автоматически. Решение — `imageOrientation: "from-image"` в
//! `createImageBitmap`.
//!
//! ## resize при загрузке
//!
//! `resizeWidth` / `resizeHeight` масштабируют при декодировании,
//! без промежуточного canvas. Для больших файлов это экономит
//! и время, и память.

use js_sys::{Object, Reflect};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{Blob, HtmlImageElement, ImageBitmap, Url};

/// Результат загрузки: `<img>` и `ImageBitmap` — оба декодированы
/// из одного Blob. Используется в демо для сравнения.
pub struct LoadedPair {
    pub img: HtmlImageElement,
    pub bitmap: ImageBitmap,
    pub img_ms: f64,
    pub bitmap_ms: f64,
    pub img_w: u32,
    pub img_h: u32,
    pub bitmap_w: u32,
    pub bitmap_h: u32,
}

/// Загружает Blob как `<img>` и ждёт `decode()`.
///
/// `img.decode()` — Promise, резолвится, когда картинка **полностью
/// декодирована** и готова к синхронному рисованию. В отличие от
/// `onload`, работает даже если картинка уже в кэше.
///
/// Object URL отзывается до возврата — картинка уже в памяти браузера.
pub async fn load_as_image(blob: &Blob) -> Result<HtmlImageElement, JsValue> {
    let document = web_sys::window()
        .ok_or_else(|| JsValue::from_str("no window"))?
        .document()
        .ok_or_else(|| JsValue::from_str("no document"))?;

    let img: HtmlImageElement = document
        .create_element("img")?
        .dyn_into::<HtmlImageElement>()?;

    let url = Url::create_object_url_with_blob(blob)?;
    img.set_src(&url);

    let result = wasm_bindgen_futures::JsFuture::from(img.decode()).await;
    let _ = Url::revoke_object_url(&url);
    result?;
    Ok(img)
}

/// Опции для `createImageBitmap`.
#[derive(Clone, Copy, Debug)]
pub struct BitmapOptions {
    /// Применять EXIF-ориентацию (`"from-image"`).
    pub apply_exif: bool,
    /// Максимальный размер стороны. `None` — без resize.
    pub max_dim: Option<u32>,
    /// Качество resize: `"low"`, `"medium"`, `"high"`, `"pixelated"`.
    pub quality: &'static str,
}

impl Default for BitmapOptions {
    fn default() -> Self {
        Self {
            apply_exif: true,
            max_dim: None,
            quality: "high",
        }
    }
}

/// Декодирует Blob в `ImageBitmap` через `createImageBitmap`.
///
/// Вызываем через `Reflect`, потому что web-sys не всегда экспортирует
/// `createImageBitmap` с нужной сигнатурой. Универсальный обход.
pub async fn load_as_bitmap(
    blob: &Blob,
    options: BitmapOptions,
) -> Result<ImageBitmap, JsValue> {
    let global = js_sys::global();
    let func_value = Reflect::get(&global, &JsValue::from_str("createImageBitmap"))?;
    if !func_value.is_function() {
        return Err(JsValue::from_str(
            "createImageBitmap not available in this browser",
        ));
    }
    let func: js_sys::Function = func_value.dyn_into()?;

    let opts = Object::new();

    if options.apply_exif {
        Reflect::set(
            &opts,
            &JsValue::from_str("imageOrientation"),
            &JsValue::from_str("from-image"),
        )?;
    }

    if let Some(max_dim) = options.max_dim {
        // Уменьшаем пропорционально. `resizeWidth` / `resizeHeight`
        // работают вместе: браузер сохраняет aspect ratio сам,
        // если указать оба, но можно указать одно — тогда вторая
        // сторона считается автоматически.
        Reflect::set(
            &opts,
            &JsValue::from_str("resizeWidth"),
            &JsValue::from_f64(max_dim as f64),
        )?;
        Reflect::set(
            &opts,
            &JsValue::from_str("resizeHeight"),
            &JsValue::from_f64(max_dim as f64),
        )?;
        Reflect::set(
            &opts,
            &JsValue::from_str("resizeQuality"),
            &JsValue::from_str(options.quality),
        )?;
    }

    // createImageBitmap(blob, options) → Promise<ImageBitmap>.
    let promise: js_sys::Promise = func
        .call2(&global, blob.as_ref(), &opts)?
        .dyn_into()?;
    let value = wasm_bindgen_futures::JsFuture::from(promise).await?;
    value.dyn_into::<ImageBitmap>()
}

/// Замеряет время выполнения асинхронной операции в миллисекундах.
/// Возвращает `(результат, elapsed_ms)`.
pub async fn timed<F, T>(fut: F) -> (Result<T, JsValue>, f64)
where
    F: std::future::Future<Output = Result<T, JsValue>>,
{
    let t0 = js_sys::Date::now();
    let result = fut.await;
    let elapsed = js_sys::Date::now() - t0;
    (result, elapsed)
}

/// Загружает Blob двумя способами параллельно, замеряет тайминги.
/// Используется в демо, чтобы показать разницу на одном экране.
pub async fn load_both(
    blob: &Blob,
    options: BitmapOptions,
) -> Result<LoadedPair, JsValue> {
    let (img_result, img_ms) = timed(load_as_image(blob)).await;
    let img = img_result?;

    let (bitmap_result, bitmap_ms) = timed(load_as_bitmap(blob, options)).await;
    let bitmap = bitmap_result?;

    let img_w = img.natural_width();
    let img_h = img.natural_height();
    let bitmap_w = bitmap.width();
    let bitmap_h = bitmap.height();

    Ok(LoadedPair {
        img,
        bitmap,
        img_ms,
        bitmap_ms,
        img_w,
        img_h,
        bitmap_w,
        bitmap_h,
    })
}

/// Закрывает `ImageBitmap`, освобождая память. Идемпотентно —
/// повторный вызов не паникует.
pub fn close_bitmap(bitmap: &ImageBitmap) {
    bitmap.close();
}