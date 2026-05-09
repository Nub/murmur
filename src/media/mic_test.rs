use crate::media::voice::{find_input_device, find_supported_input_config};
use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::SampleFormat;
use std::sync::{Arc, Mutex};
use tracing::{error, info};

/// Lightweight mic tester that runs in the calling thread.
/// Captures audio and computes RMS level for a meter display.
pub struct MicTester {
    stream: Option<cpal::Stream>,
    level: Arc<Mutex<f32>>,
}

impl MicTester {
    pub fn start(device_name: &Option<String>, input_volume: f32) -> Result<Self> {
        let device = find_input_device(device_name)?;
        let supported = find_supported_input_config(&device)?;
        let format = supported.sample_format();
        let channels = supported.channels() as usize;
        let config: cpal::StreamConfig = supported.into();

        info!(
            "Mic test: {} ({}Hz, {}ch, {:?})",
            device.name().unwrap_or_default(),
            config.sample_rate.0,
            channels,
            format,
        );

        let level = Arc::new(Mutex::new(0.0f32));
        let level_ref = level.clone();
        let vol = input_volume;

        let stream = match format {
            SampleFormat::F32 => device.build_input_stream(
                &config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    compute_level(data, channels, vol, &level_ref);
                },
                |err| error!("Mic test error: {}", err),
                None,
            )?,
            SampleFormat::I16 => device.build_input_stream(
                &config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    let floats: Vec<f32> = data.iter().map(|&s| s as f32 / 32768.0).collect();
                    compute_level(&floats, channels, vol, &level_ref);
                },
                |err| error!("Mic test error: {}", err),
                None,
            )?,
            _ => return Err(anyhow!("Unsupported sample format: {:?}", format)),
        };

        stream.play()?;
        info!("Mic test started");

        Ok(Self {
            stream: Some(stream),
            level,
        })
    }

    /// Get the current RMS level (0.0 - 1.0).
    pub fn level(&self) -> f32 {
        self.level.lock().map(|l| *l).unwrap_or(0.0)
    }

    pub fn stop(&mut self) {
        self.stream = None;
        info!("Mic test stopped");
    }
}

impl Drop for MicTester {
    fn drop(&mut self) {
        self.stop();
    }
}

fn compute_level(data: &[f32], channels: usize, volume: f32, level: &Arc<Mutex<f32>>) {
    // Downmix to mono
    let mono: Vec<f32> = if channels > 1 {
        data.chunks(channels)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
            .collect()
    } else {
        data.to_vec()
    };

    if mono.is_empty() {
        return;
    }

    let rms = (mono.iter().map(|s| s * s).sum::<f32>() / mono.len() as f32).sqrt() * volume;
    // Scale and clamp for display (multiply by ~3 to make meter responsive)
    let display_level = (rms * 3.0).min(1.0);

    if let Ok(mut l) = level.lock() {
        // Smooth: fast attack, slow decay
        if display_level > *l {
            *l = *l * 0.3 + display_level * 0.7; // fast attack
        } else {
            *l = *l * 0.85 + display_level * 0.15; // slow decay
        }
    }
}
