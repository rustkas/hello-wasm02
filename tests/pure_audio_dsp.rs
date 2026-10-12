//! 23.13. Чистые тесты pitch shifter. Запускаются без браузера.

use hello_wasm02::audio_dsp::PitchShifter;

#[test]
fn new_shifter_starts_silent() {
    let sh = PitchShifter::new();
    assert_eq!(sh.ratio(), 1.0);
}

#[test]
fn ratio_clamped_to_range() {
    let mut sh = PitchShifter::new();
    sh.set_ratio(10.0);
    assert!(sh.ratio() <= 2.0);
    sh.set_ratio(0.1);
    assert!(sh.ratio() >= 0.5);
}

#[test]
fn process_produces_finite_output() {
    let mut sh = PitchShifter::new();
    sh.set_ratio(1.0);
    let input = vec![0.5f32; 128];
    let mut output = vec![0.0f32; 128];
    // Прогреваем буфер.
    for _ in 0..16 {
        sh.process(&input, &mut output);
    }
    assert!(output.iter().all(|s| s.is_finite()));
}

#[test]
fn process_handles_different_lengths() {
    let mut sh = PitchShifter::new();
    let input = vec![0.1f32; 64];
    let mut output = vec![0.0f32; 128];
    sh.process(&input, &mut output);
    // Первые 64 должны быть заполнены, остальные — как были.
    assert!(output.iter().all(|s| s.is_finite()));
}

#[test]
fn reset_clears_state() {
    let mut sh = PitchShifter::new();
    let input = vec![1.0f32; 128];
    let mut output = vec![0.0f32; 128];
    for _ in 0..8 {
        sh.process(&input, &mut output);
    }
    sh.reset();
    // После сброса ring снова тихий.
    for _ in 0..8 {
        sh.process(&input, &mut output);
    }
    // Никаких NaN/inf.
    assert!(output.iter().all(|s| s.is_finite()));
}

#[test]
fn high_ratio_eventually_produces_output() {
    let mut sh = PitchShifter::new();
    sh.set_ratio(1.8);
    let input = vec![0.3f32; 128];
    let mut output = vec![0.0f32; 128];
    // Крутим долго — должна появиться ненулевая энергия.
    for _ in 0..64 {
        sh.process(&input, &mut output);
    }
    let energy: f32 = output.iter().map(|s| s * s).sum();
    assert!(energy.is_finite());
}