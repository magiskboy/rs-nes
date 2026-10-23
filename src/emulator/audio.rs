use crate::apu::Apu;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamConfig};
use std::collections::VecDeque;
use std::f32::consts::TAU;
use std::sync::{Arc, Mutex};

/// Take all pending APU samples.
pub fn take_samples(apu: &mut Apu) -> Vec<f32> {
    apu.take_samples()
}

/// Drop buffered samples without returning them.
pub fn clear_samples(apu: &mut Apu) {
    let _ = apu.take_samples();
}

/// Short UI beeps for the NES OS shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiSfx {
    Move,
    Confirm,
    Back,
    Select,
}

/// Real-time audio output backed by cpal + a shared mono ring.
pub struct AudioOut {
    _stream: Stream,
    queue: Arc<Mutex<VecDeque<f32>>>,
    volume: Arc<Mutex<f32>>,
    pub sample_rate: u32,
}

impl AudioOut {
    /// Open the default output device (F32 preferred) and start playback.
    pub fn start() -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| "no default output device".to_string())?;

        let supported = device
            .supported_output_configs()
            .map_err(|e| e.to_string())?
            .filter(|c| c.sample_format() == SampleFormat::F32)
            .max_by_key(|c| c.max_sample_rate().0)
            .ok_or_else(|| "no F32 output config".to_string())?;

        let sample_rate = supported
            .max_sample_rate()
            .0
            .min(48_000)
            .max(supported.min_sample_rate().0);
        let config: StreamConfig = supported
            .with_sample_rate(cpal::SampleRate(sample_rate))
            .into();

        let queue = Arc::new(Mutex::new(VecDeque::with_capacity(8192)));
        let volume = Arc::new(Mutex::new(1.0_f32));
        let q_cb = Arc::clone(&queue);
        let vol_cb = Arc::clone(&volume);
        let channels = config.channels as usize;

        let stream = device
            .build_output_stream(
                &config,
                move |data: &mut [f32], _| {
                    let vol = *vol_cb.lock().unwrap();
                    let mut q = q_cb.lock().unwrap();
                    for frame in data.chunks_mut(channels) {
                        let s = q.pop_front().unwrap_or(0.0) * vol;
                        for sample in frame.iter_mut() {
                            *sample = s;
                        }
                    }
                },
                |err| eprintln!("audio stream error: {err}"),
                None,
            )
            .map_err(|e| e.to_string())?;

        stream.play().map_err(|e| e.to_string())?;

        Ok(Self {
            _stream: stream,
            queue,
            volume,
            sample_rate,
        })
    }

    /// Master volume in `0.0..=1.0` (applied on playback).
    pub fn set_volume(&self, volume: f32) {
        *self.volume.lock().unwrap() = volume.clamp(0.0, 1.0);
    }

    pub fn volume(&self) -> f32 {
        *self.volume.lock().unwrap()
    }

    /// Drop queued samples (useful when pausing so audio stops immediately).
    pub fn clear_queue(&self) {
        self.queue.lock().unwrap().clear();
    }

    /// Push mono host-rate samples from the APU into the playback ring.
    pub fn push_samples(&self, samples: &[f32]) {
        let mut q = self.queue.lock().unwrap();
        // Bound latency: drop oldest if more than ~0.25 s queued.
        let cap = (self.sample_rate as usize) / 4;
        for &s in samples {
            if q.len() >= cap {
                q.pop_front();
            }
            q.push_back(s);
        }
    }

    /// Play a short square-wave UI beep (OS shell navigation).
    pub fn play_sfx(&self, sfx: UiSfx) {
        let samples = synthesize_sfx(sfx, self.sample_rate);
        self.push_samples(&samples);
    }

    pub fn queued(&self) -> usize {
        self.queue.lock().unwrap().len()
    }
}

fn synthesize_sfx(sfx: UiSfx, sample_rate: u32) -> Vec<f32> {
    // (freq_hz, duration_ms, amplitude)
    let notes: &[(f32, f32, f32)] = match sfx {
        UiSfx::Move => &[(880.0, 35.0, 0.18)],
        UiSfx::Confirm => &[(660.0, 40.0, 0.22), (990.0, 55.0, 0.20)],
        UiSfx::Back => &[(440.0, 45.0, 0.16), (330.0, 50.0, 0.14)],
        UiSfx::Select => &[(520.0, 30.0, 0.18), (780.0, 45.0, 0.18)],
    };

    let sr = sample_rate as f32;
    let mut out = Vec::new();
    for &(freq, ms, amp) in notes {
        let n = ((ms / 1000.0) * sr).round().max(1.0) as usize;
        for i in 0..n {
            let t = i as f32 / sr;
            let env = {
                let attack = (i as f32 / (n as f32 * 0.12)).min(1.0);
                let release = 1.0 - (i as f32 / n as f32) * 0.85;
                attack * release
            };
            let phase = (t * freq * TAU).sin();
            // Soft square for a slightly NES-like click.
            let sq = if phase >= 0.0 { 1.0 } else { -1.0 };
            out.push(sq * amp * env);
        }
        // Tiny gap between notes in multi-tone SFX.
        let gap = (sr * 0.008) as usize;
        out.extend(std::iter::repeat_n(0.0, gap));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drain_empty() {
        let mut apu = Apu::new();
        assert!(take_samples(&mut apu).is_empty());
    }
}
