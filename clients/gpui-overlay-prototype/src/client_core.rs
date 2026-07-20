use serde_json::Value;

pub const OUTPUT_SAMPLE_RATE: u32 = 48_000;

#[derive(Clone, Debug, PartialEq)]
pub enum BridgeEvent {
    Phase(String),
    Partial(String),
    Text(String),
    Final(String),
    Error {
        phase: Option<String>,
        message: String,
    },
    Other,
}

pub fn decode_bridge_event(line: &str) -> Result<BridgeEvent, serde_json::Error> {
    let event: Value = serde_json::from_str(line)?;
    let event_type = event
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let text = || {
        event
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };

    Ok(match event_type {
        "status" => BridgeEvent::Phase(
            event
                .get("phase")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
        ),
        "partial" => BridgeEvent::Partial(text()),
        "text" => BridgeEvent::Text(text()),
        "final" => BridgeEvent::Final(text()),
        "error" => BridgeEvent::Error {
            phase: event
                .get("phase")
                .and_then(Value::as_str)
                .map(str::to_owned),
            message: event
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("Unknown bridge error")
                .to_owned(),
        },
        _ => BridgeEvent::Other,
    })
}

pub struct PcmNormalizer {
    channels: usize,
    step: f64,
    source_position: f64,
    pending_mono: Vec<f32>,
}

impl PcmNormalizer {
    pub fn new(input_sample_rate: u32, channels: u16) -> Result<Self, String> {
        if input_sample_rate == 0 {
            return Err("input sample rate must be greater than zero".to_string());
        }
        if channels == 0 {
            return Err("input channel count must be greater than zero".to_string());
        }

        Ok(Self {
            channels: channels as usize,
            step: input_sample_rate as f64 / OUTPUT_SAMPLE_RATE as f64,
            source_position: 0.0,
            pending_mono: Vec::new(),
        })
    }

    pub fn push_f32(&mut self, interleaved: &[f32]) -> Vec<i16> {
        for frame in interleaved.chunks_exact(self.channels) {
            let mono = frame.iter().copied().sum::<f32>() / self.channels as f32;
            self.pending_mono.push(mono.clamp(-1.0, 1.0));
        }

        let mut output = Vec::with_capacity(
            ((self.pending_mono.len() as f64 / self.step).ceil() as usize).saturating_add(1),
        );
        while self.source_position + 1.0 < self.pending_mono.len() as f64 {
            let index = self.source_position.floor() as usize;
            let fraction = (self.source_position - index as f64) as f32;
            let sample = self.pending_mono[index]
                + (self.pending_mono[index + 1] - self.pending_mono[index]) * fraction;
            output.push(float_to_i16(sample));
            self.source_position += self.step;
        }

        let consumed = self.source_position.floor() as usize;
        if consumed > 0 {
            self.pending_mono.drain(..consumed);
            self.source_position -= consumed as f64;
        }
        output
    }
}

pub fn pcm_level(samples: &[i16]) -> Option<(f32, f32)> {
    if samples.is_empty() {
        return None;
    }

    let mut sum_squares = 0.0f64;
    let mut peak = 0i32;
    for sample in samples {
        let value = i32::from(*sample);
        sum_squares += f64::from(value * value);
        peak = peak.max(value.abs());
    }
    let rms = (sum_squares / samples.len() as f64).sqrt();
    let to_dbfs = |value: f64| {
        if value > 0.0 {
            (20.0 * (value / 32_768.0).log10()).max(-120.0) as f32
        } else {
            -120.0
        }
    };
    Some((to_dbfs(rms), to_dbfs(peak as f64)))
}

pub fn encode_s16le(samples: &[i16]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

fn float_to_i16(sample: f32) -> i16 {
    let scaled = if sample >= 0.0 {
        sample * i16::MAX as f32
    } else {
        sample * -(i16::MIN as f32)
    };
    scaled.round().clamp(i16::MIN as f32, i16::MAX as f32) as i16
}

#[cfg(test)]
mod tests {
    use super::{
        BridgeEvent, OUTPUT_SAMPLE_RATE, PcmNormalizer, decode_bridge_event, encode_s16le,
        pcm_level,
    };

    #[test]
    fn decodes_bridge_status_and_text_events() {
        assert_eq!(
            decode_bridge_event(r#"{"type":"status","phase":"recording"}"#).unwrap(),
            BridgeEvent::Phase("recording".to_string())
        );
        assert_eq!(
            decode_bridge_event(r#"{"type":"final","text":"hello"}"#).unwrap(),
            BridgeEvent::Final("hello".to_string())
        );
    }

    #[test]
    fn ignores_unknown_events_without_rejecting_the_protocol_line() {
        assert_eq!(
            decode_bridge_event(r#"{"type":"hello","authRequired":false}"#).unwrap(),
            BridgeEvent::Other
        );
    }

    #[test]
    fn downmixes_stereo_and_encodes_little_endian_pcm() {
        let mut normalizer = PcmNormalizer::new(OUTPUT_SAMPLE_RATE, 2).unwrap();
        let samples = normalizer.push_f32(&[1.0, -1.0, 0.5, 0.5, -0.5, -0.5]);

        assert_eq!(samples, vec![0, 16_384]);
        assert_eq!(encode_s16le(&samples), vec![0, 0, 0, 64]);
    }

    #[test]
    fn resampling_state_is_continuous_across_callback_chunks() {
        let input_rate = 44_100;
        let frames = input_rate as usize;
        let source: Vec<f32> = (0..frames)
            .map(|index| (index as f32 / 100.0).sin() * 0.5)
            .collect();
        let mut normalizer = PcmNormalizer::new(input_rate, 1).unwrap();
        let mut output = Vec::new();
        for chunk in source.chunks(137) {
            output.extend(normalizer.push_f32(chunk));
        }

        assert!((output.len() as isize - OUTPUT_SAMPLE_RATE as isize).abs() <= 2);
        assert!(output.windows(2).any(|pair| pair[0] != pair[1]));
    }

    #[test]
    fn level_meter_distinguishes_silence_from_signal() {
        assert_eq!(pcm_level(&[0; 16]), Some((-120.0, -120.0)));
        let (rms, peak) = pcm_level(&[16_384, -16_384]).unwrap();
        assert!((-6.1..-5.9).contains(&rms));
        assert!((-6.1..-5.9).contains(&peak));
    }
}
