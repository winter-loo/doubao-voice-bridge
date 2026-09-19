//! Regression for Apple-style Liquid Glass readability over bright text-heavy scenes.
//! The generated fixture is deterministic, contains no user pixels and never opens a window.
//! It does not claim to reproduce Apple's private shader or parameters.

use crate::{
    ensure,
    gpu::{self, AdaptivePipeline},
    voice_model::Phase,
    AppResult,
};
use std::path::Path;

const W: u32 = 162;
const H: u32 = 39;
const STEADY_TIME: f32 = 0.8;

// Engineering bounds derived from the observed failure mode:
// preserve low-frequency environment, suppress readable dark glyph structure,
// and keep the original colored waveform distinguishable.
const MAX_TEXT_EDGE_RATIO: f64 = 0.20;
const MIN_DARK_GLYPH_MEAN: f64 = 205.0;
const MIN_MATERIAL_MEAN: f64 = 205.0;
const MAX_MATERIAL_MEAN: f64 = 238.0;
const MIN_LOW_FREQUENCY_STDDEV: f64 = 8.0;
const MIN_WAVEFORM_MEDIAN_CONTRAST: f64 = 1.40;

fn rgba_pixel(bytes: &[u8], width: u32, x: u32, y: u32) -> &[u8] {
    &bytes[((y * width + x) * 4) as usize..][..4]
}

fn encoded_luma(pixel: &[u8]) -> f64 {
    pixel[0] as f64 * 0.2126 + pixel[1] as f64 * 0.7152 + pixel[2] as f64 * 0.0722
}

fn linear_channel(value: u8) -> f64 {
    let value = value as f64 / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_luma(pixel: &[u8]) -> f64 {
    linear_channel(pixel[0]) * 0.2126
        + linear_channel(pixel[1]) * 0.7152
        + linear_channel(pixel[2]) * 0.0722
}

fn contrast(a: f64, b: f64) -> f64 {
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

fn readability_band(x: u32, y: u32) -> bool {
    x >= 10
        && x < W - 10
        && ((4..14).contains(&y) || (H - 14..H - 4).contains(&y))
}

fn edge_energy(values: &[f64]) -> f64 {
    let mut sum = 0.0;
    let mut count = 0usize;
    for y in 0..H {
        for x in 0..W {
            if !readability_band(x, y) {
                continue;
            }
            let here = values[(y * W + x) as usize];
            if x + 1 < W && readability_band(x + 1, y) {
                sum += (here - values[(y * W + x + 1) as usize]).abs();
                count += 1;
            }
            if y + 1 < H && readability_band(x, y + 1) {
                sum += (here - values[((y + 1) * W + x) as usize]).abs();
                count += 1;
            }
        }
    }
    if count == 0 { 0.0 } else { sum / count as f64 }
}

fn local_mean(values: &[f64], cx: u32, cy: u32, radius: u32) -> f64 {
    let mut sum = 0.0;
    let mut count = 0usize;
    for y in cy.saturating_sub(radius)..=(cy + radius).min(H - 1) {
        for x in cx.saturating_sub(radius)..=(cx + radius).min(W - 1) {
            sum += values[(y * W + x) as usize];
            count += 1;
        }
    }
    sum / count as f64
}

fn low_frequency_stddev(values: &[f64]) -> f64 {
    let mut samples = Vec::new();
    for &y in &[7u32, 11, H - 12, H - 8] {
        let mut x = 16u32;
        while x < W - 16 {
            samples.push(local_mean(values, x, y, 3));
            x += 6;
        }
    }
    let mean = samples.iter().sum::<f64>() / samples.len() as f64;
    (samples.iter().map(|value| (value - mean).powi(2)).sum::<f64>()
        / samples.len() as f64)
        .sqrt()
}

pub(crate) unsafe fn generated_white_text_fixture(raw_width: u32, raw_height: u32) -> AppResult<Vec<u8>> {
    // GDI rasterizes our own deterministic Chinese label; it never reads the desktop.
    let coverage = crate::desktop::text_mask_for_label(
        raw_width,
        raw_height,
        2.0,
        false,
        "继续追问 继续追问",
        false,
    )?;
    ensure(
        coverage.iter().filter(|value| **value >= 192).count() > 500,
        "White-text fixture did not contain enough opaque glyph coverage",
    )?;
    let mut bgra = vec![0u8; (raw_width * raw_height * 4) as usize];
    for (index, alpha) in coverage.into_iter().enumerate() {
        let value = 255u8.saturating_sub(alpha);
        bgra[index * 4..index * 4 + 4].copy_from_slice(&[value, value, value, 255]);
    }
    Ok(bgra)
}

unsafe fn advance(
    pipeline: &AdaptivePipeline,
    from: f32,
    to: f32,
    phase: Phase,
    level: f32,
) {
    let mut time = from;
    while time + 1.0 / 120.0 < to {
        time += 1.0 / 120.0;
        pipeline.render_voice(false, time, phase, level, 1.0);
    }
    pipeline.render_voice(false, to, phase, level, 1.0);
}

fn source_body_luma(fixture: &[u8], raw_width: u32, padding: u32) -> Vec<f64> {
    let mut values = Vec::with_capacity((W * H) as usize);
    for y in 0..H {
        for x in 0..W {
            let p = rgba_pixel(fixture, raw_width, padding + x, padding + y);
            // The generated fixture is grayscale BGRA, so channel order is irrelevant.
            values.push(p[0] as f64);
        }
    }
    values
}

fn output_luma(bytes: &[u8]) -> Vec<f64> {
    let mut values = Vec::with_capacity((W * H) as usize);
    for y in 0..H {
        for x in 0..W {
            values.push(encoded_luma(rgba_pixel(bytes, W, x, y)));
        }
    }
    values
}

fn write_source_png(path: &Path, fixture: &[u8], width: u32, height: u32) -> AppResult<()> {
    let mut rgba = fixture.to_vec();
    for pixel in rgba.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    crate::png::write(path, width, height, &rgba)?;
    Ok(())
}

pub(crate) unsafe fn verify(directory: Option<&Path>) -> AppResult<()> {
    let (device, context) = gpu::create_device(None)?;
    let mut pipeline = AdaptivePipeline::new_voice(
        device,
        context,
        W,
        H,
        &vec![0u8; (W * H) as usize],
    )?;
    let fixture = generated_white_text_fixture(pipeline.raw.width, pipeline.raw.height)?;
    crate::voice_render_checks::upload_fixture_checked(&pipeline, &fixture)?;

    // Listening at level zero leaves only the three-pixel idle strokes in the
    // center. The two analysis bands stay free of foreground content.
    advance(&pipeline, 0.0, STEADY_TIME, Phase::Listening, 0.0);
    let material = pipeline.read_rgba(&pipeline.output)?;
    let source = source_body_luma(&fixture, pipeline.raw.width, pipeline.padding);
    let output = output_luma(&material);

    let source_edges = edge_energy(&source);
    let output_edges = edge_energy(&output);
    ensure(source_edges > 4.0, "White-text fixture has insufficient edge energy")?;
    let edge_ratio = output_edges / source_edges;

    let mut dark_sum = 0.0;
    let mut dark_count = 0usize;
    let mut material_sum = 0.0;
    let mut material_count = 0usize;
    for y in 0..H {
        for x in 0..W {
            if !readability_band(x, y) {
                continue;
            }
            let index = (y * W + x) as usize;
            material_sum += output[index];
            material_count += 1;
            if source[index] < 64.0 {
                dark_sum += output[index];
                dark_count += 1;
            }
        }
    }
    ensure(dark_count > 250, "White-text fixture has too few dark glyph samples")?;
    let dark_mean = dark_sum / dark_count as f64;
    let material_mean = material_sum / material_count as f64;
    let low_stddev = low_frequency_stddev(&output);

    // Same material state, then replace only the actual audio level.
    advance(&pipeline, STEADY_TIME, 1.0, Phase::Listening, 0.0);
    let quiet = pipeline.read_rgba(&pipeline.output)?;
    pipeline.render_voice(false, 1.0, Phase::Listening, 1.0, 1.0);
    let loud = pipeline.read_rgba(&pipeline.output)?;
    let scale = H as f32 / 26.0;
    let center_y = H / 2;
    let mut waveform_contrasts = Vec::new();
    for index in 0..20 {
        let x = ((W as f32 - 78.0 * scale) * 0.5
            + (index as f32 * 4.0 + 1.0) * scale)
            .floor() as u32;
        let foreground = linear_luma(rgba_pixel(&loud, W, x, center_y));
        // Estimate the local material from equal-distance samples above and below
        // the idle center strokes. This avoids inventing a second material render.
        let above = linear_luma(rgba_pixel(&quiet, W, x, center_y - 6));
        let below = linear_luma(rgba_pixel(&quiet, W, x, center_y + 6));
        waveform_contrasts.push(contrast(foreground, (above + below) * 0.5));
    }
    waveform_contrasts.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let waveform_median =
        (waveform_contrasts[waveform_contrasts.len() / 2 - 1]
            + waveform_contrasts[waveform_contrasts.len() / 2])
            * 0.5;

    let mut failures = Vec::new();
    if edge_ratio > MAX_TEXT_EDGE_RATIO {
        failures.push(format!(
            "background glyph edge ratio {edge_ratio:.4} exceeds {MAX_TEXT_EDGE_RATIO:.2}"
        ));
    }
    if dark_mean < MIN_DARK_GLYPH_MEAN {
        failures.push(format!(
            "dark glyph mean {dark_mean:.3} remains below {MIN_DARK_GLYPH_MEAN:.1}"
        ));
    }
    if material_mean < MIN_MATERIAL_MEAN || material_mean > MAX_MATERIAL_MEAN {
        failures.push(format!(
            "material band mean {material_mean:.3} is outside \
             [{MIN_MATERIAL_MEAN:.1}, {MAX_MATERIAL_MEAN:.1}]"
        ));
    }
    if low_stddev < MIN_LOW_FREQUENCY_STDDEV {
        failures.push(format!(
            "low-frequency variation {low_stddev:.3} is below \
             {MIN_LOW_FREQUENCY_STDDEV:.1}; the result risks becoming an opaque card"
        ));
    }
    if waveform_median < MIN_WAVEFORM_MEDIAN_CONTRAST {
        failures.push(format!(
            "waveform median contextual contrast {waveform_median:.4} is below \
             {MIN_WAVEFORM_MEDIAN_CONTRAST:.2}"
        ));
    }

    let metrics = format!(
        concat!(
            "{{\n",
            "  \"scope\": \"actual production HLSL on WARP; generated white background ",
            "with GDI-rasterized black text; no window/capture/audio\",\n",
            "  \"principle\": \"retain low-frequency environment while suppressing ",
            "background text readability and protecting foreground\",\n",
            "  \"source_edge_energy\": {source_edges:.6},\n",
            "  \"output_edge_energy\": {output_edges:.6},\n",
            "  \"edge_ratio\": {edge_ratio:.6},\n",
            "  \"dark_glyph_mean\": {dark_mean:.6},\n",
            "  \"material_mean\": {material_mean:.6},\n",
            "  \"low_frequency_stddev\": {low_stddev:.6},\n",
            "  \"waveform_median_contextual_contrast\": {waveform_median:.6}\n",
            "}}\n"
        ),
        source_edges = source_edges,
        output_edges = output_edges,
        edge_ratio = edge_ratio,
        dark_mean = dark_mean,
        material_mean = material_mean,
        low_stddev = low_stddev,
        waveform_median = waveform_median,
    );

    if let Some(directory) = directory {
        std::fs::create_dir_all(directory)?;
        write_source_png(
            &directory.join("source-white-black-text.png"),
            &fixture,
            pipeline.raw.width,
            pipeline.raw.height,
        )?;
        pipeline.render_voice(false, 1.0, Phase::Listening, 0.0, 1.0);
        crate::png::write(
            &directory.join("steady-material.png"),
            pipeline.raw.width,
            pipeline.raw.height,
            &pipeline.composite_fixture()?,
        )?;
        pipeline.render_voice(false, 1.0, Phase::Listening, 1.0, 1.0);
        crate::png::write(
            &directory.join("listening-waveform.png"),
            pipeline.raw.width,
            pipeline.raw.height,
            &pipeline.composite_fixture()?,
        )?;
        let mask = crate::voice_window::content_mask(W, H, scale, Phase::Optimizing)?;
        pipeline.set_voice_mask(&mask)?;
        pipeline.render_voice(false, 1.2, Phase::Optimizing, 0.0, 1.0);
        crate::png::write(
            &directory.join("optimizing-text.png"),
            pipeline.raw.width,
            pipeline.raw.height,
            &pipeline.composite_fixture()?,
        )?;
        std::fs::write(directory.join("metrics.json"), &metrics)?;
        std::fs::write(
            directory.join("result.txt"),
            if failures.is_empty() {
                "PASS\n".to_string()
            } else {
                format!("FAIL:\n{}\n", failures.join("\n"))
            },
        )?;
    }

    eprintln!(
        "[white-text-readability] edge_ratio={edge_ratio:.4}; dark_mean={dark_mean:.3}; \
         material_mean={material_mean:.3}; low_frequency_stddev={low_stddev:.3}; \
         waveform_median_contrast={waveform_median:.4}"
    );
    ensure(
        failures.is_empty(),
        &format!(
            "White+black-text Liquid Glass regression failed:\n{}\n\
             Preserve low-frequency environment; suppress readable background glyphs; \
             keep foreground visible without turning the capsule into an opaque card.",
            failures.join("\n")
        ),
    )?;
    eprintln!(
        "[white-text-readability] PASS; generated white+black-text fixture; \
         background glyph readability suppressed; low-frequency environment retained; \
         waveform contextual contrast protected; no window/capture/audio"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn white_text_background_is_environment_not_competing_content() {
        unsafe {
            let directory = std::env::var_os("GLASS_MATERIAL_FIXTURES")
                .map(|path| std::path::PathBuf::from(path).with_extension("white-text"));
            super::verify(directory.as_deref()).unwrap();
        }
    }
}
