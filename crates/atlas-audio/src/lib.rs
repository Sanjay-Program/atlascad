#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("Channel count mismatch: expected {0}, got {1}")]
    ChannelMismatch(usize, usize),
    #[error("Sample rate mismatch: {0} vs {1}")]
    SampleRateMismatch(u32, u32),
}

/// Planar multi-channel audio buffer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioBuffer {
    pub sample_rate: u32,
    pub channels: Vec<Vec<f64>>,
}

impl AudioBuffer {
    pub fn new(sample_rate: u32, channel_count: usize, frame_count: usize) -> Self {
        Self {
            sample_rate,
            channels: vec![vec![0.0; frame_count]; channel_count],
        }
    }

    pub fn frame_count(&self) -> usize {
        self.channels.first().map(|c| c.len()).unwrap_or(0)
    }

    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }
}

/// Biquad filter types based on Robert Bristow-Johnson's Audio EQ Cookbook
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BiquadFilterType {
    LowPass,
    HighPass,
    Peaking,
    LowShelf,
    HighShelf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiquadFilter {
    pub filter_type: BiquadFilterType,
    pub freq_hz: f64,
    pub q: f64,
    pub gain_db: f64,
    // Coefficients
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    // State memory (x1, x2, y1, y2)
    s1: f64,
    s2: f64,
}

impl BiquadFilter {
    pub fn new(filter_type: BiquadFilterType, freq_hz: f64, q: f64, gain_db: f64, sample_rate: u32) -> Self {
        let mut filter = Self {
            filter_type,
            freq_hz,
            q: q.max(0.1),
            gain_db,
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            s1: 0.0,
            s2: 0.0,
        };
        filter.calculate_coefficients(sample_rate);
        filter
    }

    pub fn calculate_coefficients(&mut self, sample_rate: u32) {
        let sr = sample_rate as f64;
        let w0 = 2.0 * std::f64::consts::PI * (self.freq_hz / sr).clamp(0.001, 0.49);
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * self.q);
        let a_amp = 10.0f64.powf(self.gain_db / 40.0);

        let (b0, b1, b2, a0, a1, a2) = match self.filter_type {
            BiquadFilterType::LowPass => {
                let b1 = 1.0 - cos_w0;
                let b0 = b1 * 0.5;
                let b2 = b0;
                let a0 = 1.0 + alpha;
                let a1 = -2.0 * cos_w0;
                let a2 = 1.0 - alpha;
                (b0, b1, b2, a0, a1, a2)
            }
            BiquadFilterType::HighPass => {
                let b1 = -(1.0 + cos_w0);
                let b0 = (1.0 + cos_w0) * 0.5;
                let b2 = b0;
                let a0 = 1.0 + alpha;
                let a1 = -2.0 * cos_w0;
                let a2 = 1.0 - alpha;
                (b0, b1, b2, a0, a1, a2)
            }
            BiquadFilterType::Peaking => {
                let b0 = 1.0 + alpha * a_amp;
                let b1 = -2.0 * cos_w0;
                let b2 = 1.0 - alpha * a_amp;
                let a0 = 1.0 + alpha / a_amp;
                let a1 = -2.0 * cos_w0;
                let a2 = 1.0 - alpha / a_amp;
                (b0, b1, b2, a0, a1, a2)
            }
            BiquadFilterType::LowShelf => {
                let two_sqrt_a_alpha = 2.0 * a_amp.sqrt() * alpha;
                let b0 = a_amp * ((a_amp + 1.0) - (a_amp - 1.0) * cos_w0 + two_sqrt_a_alpha);
                let b1 = 2.0 * a_amp * ((a_amp - 1.0) - (a_amp + 1.0) * cos_w0);
                let b2 = a_amp * ((a_amp + 1.0) - (a_amp - 1.0) * cos_w0 - two_sqrt_a_alpha);
                let a0 = (a_amp + 1.0) + (a_amp - 1.0) * cos_w0 + two_sqrt_a_alpha;
                let a1 = -2.0 * ((a_amp - 1.0) + (a_amp + 1.0) * cos_w0);
                let a2 = (a_amp + 1.0) + (a_amp - 1.0) * cos_w0 - two_sqrt_a_alpha;
                (b0, b1, b2, a0, a1, a2)
            }
            BiquadFilterType::HighShelf => {
                let two_sqrt_a_alpha = 2.0 * a_amp.sqrt() * alpha;
                let b0 = a_amp * ((a_amp + 1.0) + (a_amp - 1.0) * cos_w0 + two_sqrt_a_alpha);
                let b1 = -2.0 * a_amp * ((a_amp - 1.0) + (a_amp + 1.0) * cos_w0);
                let b2 = a_amp * ((a_amp + 1.0) + (a_amp - 1.0) * cos_w0 - two_sqrt_a_alpha);
                let a0 = (a_amp + 1.0) - (a_amp - 1.0) * cos_w0 + two_sqrt_a_alpha;
                let a1 = 2.0 * ((a_amp - 1.0) - (a_amp + 1.0) * cos_w0);
                let a2 = (a_amp + 1.0) - (a_amp - 1.0) * cos_w0 - two_sqrt_a_alpha;
                (b0, b1, b2, a0, a1, a2)
            }
        };

        let inv_a0 = 1.0 / a0;
        self.b0 = b0 * inv_a0;
        self.b1 = b1 * inv_a0;
        self.b2 = b2 * inv_a0;
        self.a1 = a1 * inv_a0;
        self.a2 = a2 * inv_a0;
    }

    pub fn process_sample(&mut self, x: f64) -> f64 {
        // Direct Form II Transposed
        let y = self.b0 * x + self.s1;
        self.s1 = self.b1 * x - self.a1 * y + self.s2;
        self.s2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// Dynamic range compressor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynamicCompressor {
    pub threshold_db: f64,
    pub ratio: f64,
    pub attack_ms: f64,
    pub release_ms: f64,
    pub makeup_db: f64,
    envelope: f64,
}

impl DynamicCompressor {
    pub fn new(threshold_db: f64, ratio: f64, attack_ms: f64, release_ms: f64, makeup_db: f64) -> Self {
        Self {
            threshold_db,
            ratio: ratio.max(1.0),
            attack_ms: attack_ms.max(0.1),
            release_ms: release_ms.max(1.0),
            makeup_db,
            envelope: 0.0,
        }
    }

    pub fn process_sample(&mut self, sample: f64, sample_rate: u32) -> f64 {
        let abs_s = sample.abs();
        let sr = sample_rate as f64;
        let attack_coeff = (-1.0 / (self.attack_ms * 0.001 * sr)).exp();
        let release_coeff = (-1.0 / (self.release_ms * 0.001 * sr)).exp();

        if abs_s > self.envelope {
            self.envelope = attack_coeff * self.envelope + (1.0 - attack_coeff) * abs_s;
        } else {
            self.envelope = release_coeff * self.envelope + (1.0 - release_coeff) * abs_s;
        }

        let env_db = if self.envelope > 1e-6 {
            20.0 * self.envelope.log10()
        } else {
            -120.0
        };

        let gain_reduction_db = if env_db > self.threshold_db {
            (self.threshold_db - env_db) * (1.0 - 1.0 / self.ratio)
        } else {
            0.0
        };

        let total_gain_db = gain_reduction_db + self.makeup_db;
        let linear_gain = 10.0f64.powf(total_gain_db / 20.0);

        sample * linear_gain
    }
}

/// Track strip in the audio mixer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioTrackStrip {
    pub id: String,
    pub name: String,
    pub volume_db: f64,
    pub pan: f64, // -1.0 (Left) to +1.0 (Right)
    pub muted: bool,
    pub solo: bool,
}

impl AudioTrackStrip {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            volume_db: 0.0,
            pan: 0.0,
            muted: false,
            solo: false,
        }
    }
}

/// Fairlight-class Multi-Track Audio Mixer
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AudioMixer {
    pub strips: Vec<AudioTrackStrip>,
    pub master_volume_db: f64,
}

impl AudioMixer {
    pub fn new() -> Self {
        Self {
            strips: Vec::new(),
            master_volume_db: 0.0,
        }
    }

    pub fn add_track(&mut self, name: &str) -> String {
        let id = format!("audio_ch_{}", self.strips.len() + 1);
        self.strips.push(AudioTrackStrip::new(id.clone(), name));
        id
    }

    /// Mix multiple track buffers down to a master stereo AudioBuffer
    pub fn mix_to_stereo(&self, track_buffers: &[&AudioBuffer]) -> Result<AudioBuffer, AudioError> {
        if track_buffers.is_empty() {
            return Ok(AudioBuffer::new(48000, 2, 0));
        }

        let sample_rate = track_buffers[0].sample_rate;
        let frame_count = track_buffers[0].frame_count();

        let mut master = AudioBuffer::new(sample_rate, 2, frame_count);
        let master_gain = 10.0f64.powf(self.master_volume_db / 20.0);

        for (idx, buf) in track_buffers.iter().enumerate() {
            if buf.sample_rate != sample_rate {
                return Err(AudioError::SampleRateMismatch(sample_rate, buf.sample_rate));
            }
            if idx >= self.strips.len() {
                break;
            }

            let strip = &self.strips[idx];
            if strip.muted {
                continue;
            }

            let track_gain = 10.0f64.powf(strip.volume_db / 20.0);
            let pan_l = ((1.0 - strip.pan) * 0.5).clamp(0.0, 1.0);
            let pan_r = ((1.0 + strip.pan) * 0.5).clamp(0.0, 1.0);

            let src_l = buf.channels.first().map(|v| v.as_slice()).unwrap_or(&[]);
            let src_r = buf.channels.get(1).map(|v| v.as_slice()).unwrap_or(src_l);

            for f in 0..frame_count {
                let sl = src_l.get(f).copied().unwrap_or(0.0);
                let sr = src_r.get(f).copied().unwrap_or(0.0);

                master.channels[0][f] += sl * track_gain * pan_l;
                master.channels[1][f] += sr * track_gain * pan_r;
            }
        }

        // Apply master gain and clamp to [-1.0, 1.0] brickwall
        for ch in &mut master.channels {
            for sample in ch.iter_mut() {
                *sample = (*sample * master_gain).clamp(-1.0, 1.0);
            }
        }

        Ok(master)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_biquad_filter_frequency_response() {
        let mut lpf = BiquadFilter::new(BiquadFilterType::LowPass, 1000.0, 0.707, 0.0, 48000);
        let mut output = 0.0;
        for _ in 0..100 {
            output = lpf.process_sample(1.0);
        }
        // Unit step DC gain should settle near 1.0
        assert!((output - 1.0).abs() < 0.05);
    }

    #[test]
    fn test_dynamic_compressor_gain_reduction() {
        let mut comp = DynamicCompressor::new(-20.0, 4.0, 5.0, 50.0, 0.0);
        let mut out = 0.0;
        // Feed loud signal at 0 dBFS (amplitude 1.0)
        for _ in 0..1000 {
            out = comp.process_sample(1.0, 48000);
        }
        // At 0 dBFS with -20 dBFS threshold & 4:1 ratio, gain reduction is 15 dB -> amp ~0.177
        assert!(out < 0.5, "Loud signal should be compressed");
    }

    #[test]
    fn test_audio_mixer_summing() {
        let mut mixer = AudioMixer::new();
        let _t1 = mixer.add_track("Jet Engine SFX");
        let _t2 = mixer.add_track("Musical Score");

        let mut b1 = AudioBuffer::new(48000, 1, 100);
        b1.channels[0].fill(0.3);

        let mut b2 = AudioBuffer::new(48000, 1, 100);
        b2.channels[0].fill(0.2);

        let mix = mixer.mix_to_stereo(&[&b1, &b2]);
        assert!(mix.is_ok());
        if let Ok(m) = mix {
            assert_eq!(m.channels.len(), 2);
            assert!((m.channels[0][0] - 0.25).abs() < 1e-4);
        }
    }
}
