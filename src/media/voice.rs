use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use nnnoiseless::DenoiseState;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use std::sync::atomic::{AtomicU16, AtomicU32, Ordering};
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::{MediaEngine, MIME_TYPE_OPUS};
use webrtc::api::APIBuilder;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;
use webrtc::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;
use webrtc::track::track_local::{TrackLocal, TrackLocalWriter};

const OPUS_SAMPLE_RATE: u32 = 48000;
const OPUS_CHANNELS: u16 = 1;
const OPUS_FRAME_SIZE: usize = 960; // 20ms at 48kHz
const RNNOISE_FRAME_SIZE: usize = 480; // 10ms at 48kHz

// ── Device enumeration ──────────────────────────────────────────────────────

/// List all available audio input device names.
pub fn list_input_devices() -> Vec<String> {
    let host = cpal::default_host();
    host.input_devices()
        .map(|devices| {
            devices
                .filter_map(|d| d.name().ok())
                .collect()
        })
        .unwrap_or_default()
}

/// List all available audio output device names.
pub fn list_output_devices() -> Vec<String> {
    let host = cpal::default_host();
    host.output_devices()
        .map(|devices| {
            devices
                .filter_map(|d| d.name().ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Get default input device name.
pub fn default_input_device_name() -> Option<String> {
    cpal::default_host().default_input_device()?.name().ok()
}

/// Get default output device name.
pub fn default_output_device_name() -> Option<String> {
    cpal::default_host().default_output_device()?.name().ok()
}

pub fn find_input_device(name: &Option<String>) -> Result<cpal::Device> {
    let host = cpal::default_host();
    if let Some(ref dev_name) = name {
        if let Ok(devices) = host.input_devices() {
            for d in devices {
                if d.name().ok().as_ref() == Some(dev_name) {
                    return Ok(d);
                }
            }
        }
    }
    host.default_input_device().ok_or_else(|| anyhow!("No audio input device"))
}

fn find_output_device(name: &Option<String>) -> Result<cpal::Device> {
    let host = cpal::default_host();
    if let Some(ref dev_name) = name {
        if let Ok(devices) = host.output_devices() {
            for d in devices {
                if d.name().ok().as_ref() == Some(dev_name) {
                    return Ok(d);
                }
            }
        }
    }
    host.default_output_device().ok_or_else(|| anyhow!("No audio output device"))
}

/// Find a supported f32 mono/stereo config, preferring 48kHz.
pub fn find_supported_input_config(device: &cpal::Device) -> Result<cpal::SupportedStreamConfig> {
    // Try to find a config that supports f32 and 48kHz
    if let Ok(configs) = device.supported_input_configs() {
        let mut best: Option<cpal::SupportedStreamConfig> = None;
        for range in configs {
            if range.sample_format() == SampleFormat::F32 || range.sample_format() == SampleFormat::I16 {
                let config = if range.min_sample_rate().0 <= 48000 && range.max_sample_rate().0 >= 48000 {
                    range.with_sample_rate(cpal::SampleRate(48000))
                } else {
                    range.with_max_sample_rate()
                };
                if best.is_none() || config.sample_format() == SampleFormat::F32 {
                    best = Some(config);
                }
            }
        }
        if let Some(config) = best {
            return Ok(config);
        }
    }
    // Fall back to default
    device.default_input_config().map_err(|e| anyhow!("No supported input config: {}", e))
}

fn find_supported_output_config(device: &cpal::Device) -> Result<cpal::SupportedStreamConfig> {
    if let Ok(configs) = device.supported_output_configs() {
        let mut best: Option<cpal::SupportedStreamConfig> = None;
        for range in configs {
            if range.sample_format() == SampleFormat::F32 || range.sample_format() == SampleFormat::I16 {
                let config = if range.min_sample_rate().0 <= 48000 && range.max_sample_rate().0 >= 48000 {
                    range.with_sample_rate(cpal::SampleRate(48000))
                } else {
                    range.with_max_sample_rate()
                };
                if best.is_none() || config.sample_format() == SampleFormat::F32 {
                    best = Some(config);
                }
            }
        }
        if let Some(config) = best {
            return Ok(config);
        }
    }
    device.default_output_config().map_err(|e| anyhow!("No supported output config: {}", e))
}

// ── Audio pipeline ──────────────────────────────────────────────────────────

struct AudioPipeline {
    denoiser: Box<DenoiseState<'static>>,
    encoder: opus::Encoder,
    denoise_buf: Vec<f32>,
    encode_buf: Vec<f32>,
    packet_buf: Vec<u8>,
    pub vad_probability: f32,
    pub vad_threshold: f32,
    pub muted: bool,
    pub noise_suppression: bool,
    pub input_volume: f32,
    /// Current RMS level for mic test meter (0.0 - 1.0)
    pub current_level: f32,
}

impl AudioPipeline {
    fn new() -> Result<Self> {
        let encoder = opus::Encoder::new(
            OPUS_SAMPLE_RATE,
            opus::Channels::Mono,
            opus::Application::Voip,
        )?;

        Ok(Self {
            denoiser: DenoiseState::new(),
            encoder,
            denoise_buf: Vec::with_capacity(RNNOISE_FRAME_SIZE),
            encode_buf: Vec::with_capacity(OPUS_FRAME_SIZE),
            packet_buf: vec![0u8; 4000],
            vad_probability: 0.0,
            vad_threshold: 0.5,
            muted: false,
            noise_suppression: true,
            input_volume: 1.0,
            current_level: 0.0,
        })
    }

    fn process(&mut self, samples: &[f32]) -> Vec<Vec<u8>> {
        // Calculate RMS level for the meter
        if !samples.is_empty() {
            let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
            // Smooth the level
            self.current_level = self.current_level * 0.7 + rms * self.input_volume * 3.0 * 0.3;
            self.current_level = self.current_level.min(1.0);
        }

        if self.muted {
            return vec![];
        }

        let mut packets = Vec::new();

        for &sample in samples {
            let sample = sample * self.input_volume;
            self.denoise_buf.push(sample);

            if self.denoise_buf.len() >= RNNOISE_FRAME_SIZE {
                let mut frame: Vec<f32> = self.denoise_buf.drain(..RNNOISE_FRAME_SIZE).collect();

                let output = if self.noise_suppression {
                    for s in frame.iter_mut() {
                        *s *= 32767.0;
                    }
                    let mut out = vec![0.0f32; RNNOISE_FRAME_SIZE];
                    self.vad_probability = self.denoiser.process_frame(&mut out, &frame);
                    for s in out.iter_mut() {
                        *s /= 32767.0;
                    }
                    out
                } else {
                    self.vad_probability = 1.0;
                    frame
                };

                if self.vad_probability >= self.vad_threshold {
                    self.encode_buf.extend_from_slice(&output);
                } else {
                    self.encode_buf.extend(std::iter::repeat(0.0f32).take(RNNOISE_FRAME_SIZE));
                }

                while self.encode_buf.len() >= OPUS_FRAME_SIZE {
                    let frame: Vec<f32> = self.encode_buf.drain(..OPUS_FRAME_SIZE).collect();
                    match self.encoder.encode_float(&frame, &mut self.packet_buf) {
                        Ok(len) if len > 0 => {
                            packets.push(self.packet_buf[..len].to_vec());
                        }
                        Err(e) => error!("Opus encode error: {}", e),
                        _ => {}
                    }
                }
            }
        }

        packets
    }
}

struct PlaybackPipeline {
    decoder: opus::Decoder,
    buffer: Arc<Mutex<Vec<f32>>>,
    decode_buf: Vec<f32>,
}

impl PlaybackPipeline {
    fn new(buffer: Arc<Mutex<Vec<f32>>>) -> Result<Self> {
        Ok(Self {
            decoder: opus::Decoder::new(OPUS_SAMPLE_RATE, opus::Channels::Mono)?,
            buffer,
            decode_buf: vec![0.0f32; OPUS_FRAME_SIZE * 2],
        })
    }

    fn decode_packet(&mut self, packet: &[u8]) -> Result<()> {
        let len = self.decoder.decode_float(packet, &mut self.decode_buf, false)?;
        if len > 0 {
            if let Ok(mut buf) = self.buffer.lock() {
                buf.extend_from_slice(&self.decode_buf[..len]);
                let max = (OPUS_SAMPLE_RATE as usize) / 2;
                if buf.len() > max {
                    let excess = buf.len() - max;
                    buf.drain(..excess);
                }
            }
        }
        Ok(())
    }
}

// ── Voice events ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum VoiceEvent {
    SendSdp { peer_id: String, sdp: String, is_offer: bool },
    SendIceCandidate { peer_id: String, candidate: String },
    StateChanged { peer_id: String, connected: bool },
}

// ── Voice engine ────────────────────────────────────────────────────────────

pub struct VoiceEngine {
    api: webrtc::api::API,
    connections: HashMap<String, Arc<RTCPeerConnection>>,
    local_audio_track: Option<Arc<TrackLocalStaticRTP>>,
    event_tx: mpsc::UnboundedSender<VoiceEvent>,
    muted: bool,
    deafened: bool,
    capture_stream: Option<cpal::Stream>,
    playback_stream: Option<cpal::Stream>,
    pipeline: Arc<Mutex<Option<AudioPipeline>>>,
    playback_buffer: Arc<Mutex<Vec<f32>>>,
    playback_pipeline: Option<PlaybackPipeline>,
    /// Channel for sending encoded Opus packets from capture thread to RTP writer task
    rtp_packet_tx: Option<mpsc::UnboundedSender<Vec<u8>>>,
    /// RTP sequence number (shared with writer task)
    rtp_seq: Arc<AtomicU16>,
    /// RTP timestamp counter
    rtp_ts: Arc<AtomicU32>,
}

impl VoiceEngine {
    pub fn new(event_tx: mpsc::UnboundedSender<VoiceEvent>) -> Result<Self> {
        let mut me = MediaEngine::default();
        me.register_default_codecs()?;
        let mut reg = Registry::new();
        reg = register_default_interceptors(reg, &mut me)?;
        let api = APIBuilder::new().with_media_engine(me).with_interceptor_registry(reg).build();

        Ok(Self {
            api,
            connections: HashMap::new(),
            local_audio_track: None,
            event_tx,
            muted: false,
            deafened: false,
            capture_stream: None,
            playback_stream: None,
            pipeline: Arc::new(Mutex::new(None)),
            playback_buffer: Arc::new(Mutex::new(Vec::with_capacity(48000))),
            playback_pipeline: None,
            rtp_packet_tx: None,
            rtp_seq: Arc::new(AtomicU16::new(0)),
            rtp_ts: Arc::new(AtomicU32::new(0)),
        })
    }

    /// Start capture with specific device and settings.
    pub fn start_audio_capture(
        &mut self,
        input_device_name: &Option<String>,
        output_device_name: &Option<String>,
        input_volume: f32,
        output_volume: f32,
        noise_suppression: bool,
        vad_threshold: f32,
    ) -> Result<()> {
        // Find and configure input device
        let input_device = find_input_device(input_device_name)?;
        let input_supported = find_supported_input_config(&input_device)?;
        let sample_format = input_supported.sample_format();
        let sample_rate = input_supported.sample_rate().0;
        let channels = input_supported.channels() as usize;
        let input_config: cpal::StreamConfig = input_supported.into();

        info!("Audio input: {} ({}Hz, {}ch, {:?})",
            input_device.name().unwrap_or_default(), sample_rate, channels, sample_format);

        // WebRTC track
        let audio_track = Arc::new(TrackLocalStaticRTP::new(
            RTCRtpCodecCapability {
                mime_type: MIME_TYPE_OPUS.to_owned(),
                clock_rate: OPUS_SAMPLE_RATE,
                channels: OPUS_CHANNELS,
                ..Default::default()
            },
            "audio".to_string(),
            "murmur-voice".to_string(),
        ));
        self.local_audio_track = Some(audio_track.clone());

        // Create channel for sending Opus packets from capture thread to RTP writer
        let (pkt_tx, mut pkt_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        self.rtp_packet_tx = Some(pkt_tx.clone());

        // Spawn a tokio task that reads encoded packets and writes RTP to the track
        let track_for_rtp = audio_track.clone();
        let rtp_seq = self.rtp_seq.clone();
        let rtp_ts = self.rtp_ts.clone();
        tokio::spawn(async move {
            use webrtc::rtp;
            while let Some(opus_data) = pkt_rx.recv().await {
                let seq = rtp_seq.fetch_add(1, Ordering::Relaxed);
                let ts = rtp_ts.fetch_add(OPUS_FRAME_SIZE as u32, Ordering::Relaxed);

                let rtp_packet = rtp::packet::Packet {
                    header: rtp::header::Header {
                        version: 2,
                        payload_type: 111, // Opus dynamic payload type
                        sequence_number: seq,
                        timestamp: ts,
                        ssrc: 1,
                        ..Default::default()
                    },
                    payload: opus_data.into(),
                };

                if let Err(e) = track_for_rtp.write_rtp(&rtp_packet).await {
                    // Track might not be connected yet, that's OK
                    let _ = e;
                }
            }
        });

        // Pipeline
        let mut pipe = AudioPipeline::new()?;
        pipe.input_volume = input_volume;
        pipe.noise_suppression = noise_suppression;
        pipe.vad_threshold = vad_threshold;
        *self.pipeline.lock().unwrap() = Some(pipe);

        let pipeline_ref = self.pipeline.clone();
        let needs_resample = sample_rate != OPUS_SAMPLE_RATE;
        let rate_ratio = OPUS_SAMPLE_RATE as f64 / sample_rate as f64;
        let pkt_tx_f32 = Some(pkt_tx.clone());
        let pkt_tx_i16 = Some(pkt_tx);

        // Build input stream — handle both f32 and i16 sample formats
        let stream = match sample_format {
            SampleFormat::F32 => {
                input_device.build_input_stream(
                    &input_config,
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        process_input_samples(data, channels, needs_resample, rate_ratio, &pipeline_ref, &pkt_tx_f32);
                    },
                    |err| error!("Audio input error: {}", err),
                    None,
                )?
            }
            SampleFormat::I16 => {
                input_device.build_input_stream(
                    &input_config,
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        let float_data: Vec<f32> = data.iter().map(|&s| s as f32 / 32768.0).collect();
                        process_input_samples(&float_data, channels, needs_resample, rate_ratio, &pipeline_ref, &pkt_tx_i16);
                    },
                    |err| error!("Audio input error: {}", err),
                    None,
                )?
            }
            _ => return Err(anyhow!("Unsupported sample format: {:?}", sample_format)),
        };
        stream.play()?;
        self.capture_stream = Some(stream);
        info!("Audio capture started");

        // Start playback
        self.start_playback(output_device_name, output_volume)?;
        Ok(())
    }

    fn start_playback(&mut self, output_device_name: &Option<String>, volume: f32) -> Result<()> {
        let output_device = find_output_device(output_device_name)?;
        let output_supported = find_supported_output_config(&output_device)?;
        let out_format = output_supported.sample_format();
        let out_config: cpal::StreamConfig = output_supported.into();

        info!("Audio output: {} ({:?})", output_device.name().unwrap_or_default(), out_format);

        let buf = self.playback_buffer.clone();
        let deafened = self.deafened;

        let stream = match out_format {
            SampleFormat::F32 => {
                output_device.build_output_stream(
                    &out_config,
                    move |data: &mut [f32], _| {
                        fill_output(data, &buf, deafened, volume);
                    },
                    |err| error!("Audio output error: {}", err),
                    None,
                )?
            }
            SampleFormat::I16 => {
                output_device.build_output_stream(
                    &out_config,
                    move |data: &mut [i16], _| {
                        let mut float_buf = vec![0.0f32; data.len()];
                        fill_output(&mut float_buf, &buf, deafened, volume);
                        for (out, &f) in data.iter_mut().zip(float_buf.iter()) {
                            *out = (f * 32767.0).clamp(-32768.0, 32767.0) as i16;
                        }
                    },
                    |err| error!("Audio output error: {}", err),
                    None,
                )?
            }
            _ => return Err(anyhow!("Unsupported output format: {:?}", out_format)),
        };
        stream.play()?;
        self.playback_stream = Some(stream);
        self.playback_pipeline = Some(PlaybackPipeline::new(self.playback_buffer.clone())?);
        Ok(())
    }

    /// Start mic test — captures audio and updates the level meter without encoding.
    pub fn start_mic_test(&mut self, input_device_name: &Option<String>, input_volume: f32, noise_suppression: bool) -> Result<()> {
        self.stop_audio_capture();

        let input_device = find_input_device(input_device_name)?;
        let input_supported = find_supported_input_config(&input_device)?;
        let sample_format = input_supported.sample_format();
        let channels = input_supported.channels() as usize;
        let input_config: cpal::StreamConfig = input_supported.into();
        let sample_rate = input_config.sample_rate.0;

        let mut pipe = AudioPipeline::new()?;
        pipe.input_volume = input_volume;
        pipe.noise_suppression = noise_suppression;
        pipe.muted = false;
        *self.pipeline.lock().unwrap() = Some(pipe);

        let pipeline_ref = self.pipeline.clone();
        let needs_resample = sample_rate != OPUS_SAMPLE_RATE;
        let rate_ratio = OPUS_SAMPLE_RATE as f64 / sample_rate as f64;

        let no_tx: Option<mpsc::UnboundedSender<Vec<u8>>> = None;
        let no_tx2: Option<mpsc::UnboundedSender<Vec<u8>>> = None;
        let stream = match sample_format {
            SampleFormat::F32 => {
                input_device.build_input_stream(
                    &input_config,
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        process_input_samples(data, channels, needs_resample, rate_ratio, &pipeline_ref, &no_tx);
                    },
                    |err| error!("Mic test error: {}", err),
                    None,
                )?
            }
            SampleFormat::I16 => {
                input_device.build_input_stream(
                    &input_config,
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        let float_data: Vec<f32> = data.iter().map(|&s| s as f32 / 32768.0).collect();
                        process_input_samples(&float_data, channels, needs_resample, rate_ratio, &pipeline_ref, &no_tx2);
                    },
                    |err| error!("Mic test error: {}", err),
                    None,
                )?
            }
            _ => return Err(anyhow!("Unsupported format")),
        };
        stream.play()?;
        self.capture_stream = Some(stream);
        info!("Mic test started");
        Ok(())
    }

    /// Get the current mic input level (0.0 - 1.0) for the level meter.
    pub fn mic_level(&self) -> f32 {
        if let Ok(guard) = self.pipeline.lock() {
            guard.as_ref().map(|p| p.current_level).unwrap_or(0.0)
        } else {
            0.0
        }
    }

    pub fn vad_probability(&self) -> f32 {
        if let Ok(guard) = self.pipeline.lock() {
            guard.as_ref().map(|p| p.vad_probability).unwrap_or(0.0)
        } else {
            0.0
        }
    }

    /// Update settings on a running pipeline.
    pub fn update_settings(&self, input_volume: f32, noise_suppression: bool, vad_threshold: f32) {
        if let Ok(mut guard) = self.pipeline.lock() {
            if let Some(ref mut pipe) = *guard {
                pipe.input_volume = input_volume;
                pipe.noise_suppression = noise_suppression;
                pipe.vad_threshold = vad_threshold;
            }
        }
    }

    pub fn stop_audio_capture(&mut self) {
        self.rtp_packet_tx = None; // Drops the sender, stopping the RTP writer task
        self.capture_stream = None;
        self.playback_stream = None;
        self.local_audio_track = None;
        *self.pipeline.lock().unwrap() = None;
        self.playback_pipeline = None;
    }

    pub fn receive_audio_packet(&mut self, packet: &[u8]) -> Result<()> {
        if self.deafened { return Ok(()); }
        if let Some(ref mut p) = self.playback_pipeline { p.decode_packet(packet)?; }
        Ok(())
    }

    pub async fn connect_to_peer(&mut self, peer_id: &str) -> Result<String> {
        let pc = self.create_peer_connection(peer_id).await?;
        let offer = pc.create_offer(None).await?;
        pc.set_local_description(offer.clone()).await?;
        self.connections.insert(peer_id.to_string(), pc);
        let _ = self.event_tx.send(VoiceEvent::SendSdp { peer_id: peer_id.to_string(), sdp: offer.sdp.clone(), is_offer: true });
        Ok(offer.sdp)
    }

    pub async fn handle_offer(&mut self, peer_id: &str, sdp: &str) -> Result<String> {
        let pc = self.create_peer_connection(peer_id).await?;
        pc.set_remote_description(RTCSessionDescription::offer(sdp.to_string())?).await?;
        let answer = pc.create_answer(None).await?;
        pc.set_local_description(answer.clone()).await?;
        self.connections.insert(peer_id.to_string(), pc);
        let _ = self.event_tx.send(VoiceEvent::SendSdp { peer_id: peer_id.to_string(), sdp: answer.sdp.clone(), is_offer: false });
        Ok(answer.sdp)
    }

    pub async fn handle_answer(&mut self, peer_id: &str, sdp: &str) -> Result<()> {
        if let Some(pc) = self.connections.get(peer_id) {
            pc.set_remote_description(RTCSessionDescription::answer(sdp.to_string())?).await?;
        }
        Ok(())
    }

    pub async fn add_ice_candidate(&mut self, peer_id: &str, candidate_json: &str) -> Result<()> {
        if let Some(pc) = self.connections.get(peer_id) {
            pc.add_ice_candidate(serde_json::from_str(candidate_json)?).await?;
        }
        Ok(())
    }

    pub async fn disconnect_peer(&mut self, peer_id: &str) -> Result<()> {
        if let Some(pc) = self.connections.remove(peer_id) { pc.close().await?; }
        Ok(())
    }

    pub async fn disconnect_all(&mut self) -> Result<()> {
        for (_, pc) in self.connections.drain() { let _ = pc.close().await; }
        self.stop_audio_capture();
        Ok(())
    }

    pub fn toggle_mute(&mut self) -> bool {
        self.muted = !self.muted;
        if let Ok(mut g) = self.pipeline.lock() { if let Some(ref mut p) = *g { p.muted = self.muted; } }
        self.muted
    }

    pub fn toggle_deafen(&mut self) -> bool {
        self.deafened = !self.deafened;
        self.deafened
    }

    pub fn is_muted(&self) -> bool { self.muted }
    pub fn is_deafened(&self) -> bool { self.deafened }
    pub fn peer_count(&self) -> usize { self.connections.len() }
    pub fn connected_peers(&self) -> Vec<String> { self.connections.keys().cloned().collect() }

    async fn create_peer_connection(&self, peer_id: &str) -> Result<Arc<RTCPeerConnection>> {
        let config = RTCConfiguration {
            ice_servers: vec![RTCIceServer {
                urls: vec![
                    "stun:stun.l.google.com:19302".into(),
                    "stun:stun1.l.google.com:19302".into(),
                ],
                ..Default::default()
            }],
            ..Default::default()
        };
        let pc = Arc::new(self.api.new_peer_connection(config).await?);
        if let Some(ref track) = self.local_audio_track {
            pc.add_track(Arc::clone(track) as Arc<dyn TrackLocal + Send + Sync>).await?;
        }

        let tx = self.event_tx.clone();
        let pid = peer_id.to_string();
        pc.on_ice_candidate(Box::new(move |c| {
            let tx = tx.clone(); let pid = pid.clone();
            Box::pin(async move {
                if let Some(c) = c {
                    if let Ok(j) = c.to_json() {
                        let _ = tx.send(VoiceEvent::SendIceCandidate { peer_id: pid, candidate: serde_json::to_string(&j).unwrap_or_default() });
                    }
                }
            })
        }));

        let tx2 = self.event_tx.clone();
        let pid2 = peer_id.to_string();
        pc.on_peer_connection_state_change(Box::new(move |state| {
            let tx = tx2.clone(); let pid = pid2.clone();
            Box::pin(async move {
                let _ = tx.send(VoiceEvent::StateChanged { peer_id: pid, connected: state == RTCPeerConnectionState::Connected });
            })
        }));

        let playback_buf = self.playback_buffer.clone();
        pc.on_track(Box::new(move |track, _, _| {
            let buf = playback_buf.clone();
            let codec = track.codec().capability.mime_type.clone();
            info!("Receiving remote audio track: {}", codec);

            Box::pin(async move {
                // Create Opus decoder for this incoming track
                let mut decoder = match opus::Decoder::new(48000, opus::Channels::Mono) {
                    Ok(d) => d,
                    Err(e) => {
                        error!("Failed to create Opus decoder: {}", e);
                        return;
                    }
                };
                let mut decode_buf = vec![0.0f32; 960 * 2];
                let mut rtp_buf = vec![0u8; 1500];

                loop {
                    match track.read(&mut rtp_buf).await {
                        Ok((rtp_packet, _)) => {
                            // Extract Opus payload from RTP
                            let payload_len = rtp_packet.payload.len();
                            if payload_len == 0 {
                                continue;
                            }

                            // Decode Opus to PCM
                            match decoder.decode_float(&rtp_packet.payload, &mut decode_buf, false) {
                                Ok(samples) if samples > 0 => {
                                    if let Ok(mut b) = buf.lock() {
                                        b.extend_from_slice(&decode_buf[..samples]);
                                        // Cap buffer at ~500ms to prevent latency buildup
                                        let max = 48000 / 2;
                                        if b.len() > max {
                                            let excess = b.len() - max;
                                            b.drain(..excess);
                                        }
                                    }
                                }
                                Err(e) => {
                                    // Occasional decode errors are normal (packet loss)
                                    let _ = e;
                                }
                                _ => {}
                            }
                        }
                        Err(_) => break,
                    }
                }
            })
        }));

        Ok(pc)
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn process_input_samples(
    data: &[f32],
    channels: usize,
    needs_resample: bool,
    rate_ratio: f64,
    pipeline: &Arc<Mutex<Option<AudioPipeline>>>,
    packet_tx: &Option<mpsc::UnboundedSender<Vec<u8>>>,
) {
    let mono: Vec<f32> = if channels > 1 {
        data.chunks(channels).map(|f| f.iter().sum::<f32>() / channels as f32).collect()
    } else {
        data.to_vec()
    };

    let samples = if needs_resample {
        let out_len = (mono.len() as f64 * rate_ratio) as usize;
        (0..out_len).map(|i| {
            let src = i as f64 / rate_ratio;
            let idx = src as usize;
            let frac = src - idx as f64;
            if idx + 1 < mono.len() {
                mono[idx] * (1.0 - frac as f32) + mono[idx + 1] * frac as f32
            } else {
                mono.get(idx).copied().unwrap_or(0.0)
            }
        }).collect()
    } else {
        mono
    };

    if let Ok(mut guard) = pipeline.lock() {
        if let Some(ref mut pipe) = *guard {
            let packets = pipe.process(&samples);
            // Send encoded Opus packets to the RTP writer task
            if let Some(ref tx) = packet_tx {
                for packet in packets {
                    let _ = tx.send(packet);
                }
            }
        }
    }
}

fn fill_output(data: &mut [f32], buf: &Arc<Mutex<Vec<f32>>>, deafened: bool, volume: f32) {
    if deafened { data.fill(0.0); return; }
    if let Ok(mut b) = buf.lock() {
        let avail = b.len().min(data.len());
        for i in 0..avail { data[i] = b[i] * volume; }
        if avail > 0 { b.drain(..avail); }
        data[avail..].fill(0.0);
    } else {
        data.fill(0.0);
    }
}
