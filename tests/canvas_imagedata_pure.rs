//! Host-тесты чистой логики: индексация и преобразования буфера.
//! Без браузера, миллисекунды.

use hello_wasm02::canvas_imagedata::{
    fill_rgba, invert_channel_inplace, make_pattern, pixel_index, read_channel,
    read_pixel, zero_channel_inplace,
};

#[test]
fn pixel_index_first_pixel() {
    assert_eq!(pixel_index(0, 0, 10, 0), 0);
    assert_eq!(pixel_index(0, 0, 10, 1), 1);
    assert_eq!(pixel_index(0, 0, 10, 2), 2);
    assert_eq!(pixel_index(0, 0, 10, 3), 3);
}

#[test]
fn pixel_index_row_step_is_w_times_4() {
    let w = 100;
    let i1 = pixel_index(0, 0, w, 0);
    let i2 = pixel_index(0, 1, w, 0);
    assert_eq!(i2 - i1, 100 * 4);
}

#[test]
fn pixel_index_column_step_is_4() {
    let i1 = pixel_index(0, 0, 50, 0);
    let i2 = pixel_index(1, 0, 50, 0);
    assert_eq!(i2 - i1, 4);
}

#[test]
fn pixel_index_channel_out_of_range_is_clamped() {
    // channel > 3 должен прижиматься к 3 (альфа), а не паниковать.
    assert_eq!(pixel_index(0, 0, 10, 5), pixel_index(0, 0, 10, 3));
    assert_eq!(pixel_index(0, 0, 10, 255), pixel_index(0, 0, 10, 3));
}

#[test]
fn read_pixel_returns_filled_color() {
    let w = 4;
    let h = 4;
    let mut buf = vec![0u8; (w * h * 4) as usize];
    fill_rgba(&mut buf, 100, 150, 200, 255);
    let (r, g, b, a) = read_pixel(&buf, 1, 2, w);
    assert_eq!((r, g, b, a), (100, 150, 200, 255));
}

#[test]
fn read_pixel_out_of_bounds_returns_zero() {
    let buf = vec![0u8; 4];
    let (r, g, b, a) = read_pixel(&buf, 100, 100, 4);
    assert_eq!((r, g, b, a), (0, 0, 0, 0));
}

#[test]
fn read_channel_reads_specific_byte() {
    let buf = vec![10, 20, 30, 40, 50, 60, 70, 80];
    assert_eq!(read_channel(&buf, 0, 0, 2, 0), 10);
    assert_eq!(read_channel(&buf, 0, 0, 2, 1), 20);
    assert_eq!(read_channel(&buf, 1, 0, 2, 0), 50);
    assert_eq!(read_channel(&buf, 1, 0, 2, 3), 80);
}

#[test]
fn invert_channel_inverts_only_that_channel() {
    // 2 пикселя: R=10, R=200
    let mut buf = vec![10, 20, 30, 40, 200, 50, 60, 70];
    invert_channel_inplace(&mut buf, 0); // инвертировать R
    assert_eq!(buf[0], 245); // 255 - 10
    assert_eq!(buf[1], 20); // G не тронут
    assert_eq!(buf[2], 30); // B не тронут
    assert_eq!(buf[3], 40); // A не тронут
    assert_eq!(buf[4], 55); // 255 - 200
    assert_eq!(buf[5], 50); // G не тронут
}

#[test]
fn invert_channel_twice_is_identity() {
    let mut buf = vec![100, 150, 200, 255];
    let original = buf.clone();
    invert_channel_inplace(&mut buf, 1);
    invert_channel_inplace(&mut buf, 1);
    assert_eq!(buf, original);
}

#[test]
fn zero_channel_sets_only_that_channel_to_zero() {
    let mut buf = vec![100, 150, 200, 255, 50, 60, 70, 80];
    zero_channel_inplace(&mut buf, 1); // зануляем G
    assert_eq!(buf[0], 100); // R
    assert_eq!(buf[1], 0); // G = 0
    assert_eq!(buf[2], 200); // B
    assert_eq!(buf[3], 255); // A
    assert_eq!(buf[4], 50); // R
    assert_eq!(buf[5], 0); // G
}

#[test]
fn zero_alpha_makes_every_pixel_transparent() {
    let mut buf = vec![100, 150, 200, 255];
    zero_channel_inplace(&mut buf, 3);
    assert_eq!(buf, vec![100, 150, 200, 0]);
}

#[test]
fn make_pattern_has_correct_size() {
    let p = make_pattern(64, 48);
    assert_eq!(p.len(), 64 * 48 * 4);
}

#[test]
fn make_pattern_all_alpha_255() {
    let p = make_pattern(64, 48);
    for chunk in p.chunks_exact(4) {
        assert_eq!(chunk[3], 255);
    }
}

#[test]
fn make_pattern_corner_red() {
    // В левом верхнем углу — красный.
    let p = make_pattern(100, 100);
    let (r, g, b, _) = read_pixel(&p, 5, 5, 100);
    assert!(r > 200, "R should be high, got {r}");
    assert!(g < 100, "G should be low, got {g}");
    assert!(b < 100, "B should be low, got {b}");
}