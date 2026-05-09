use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracing::info;

/// Synchronized clock for aligning audio streams across peers.
///
/// Uses NTP-style round-trip measurement to estimate each peer's clock offset.
/// All audio packets are timestamped with the sender's synced clock, and the
/// playback engine delays streams so they all align to a common reference time.
///
/// This enables synchronized singing / music playback across peers — everyone
/// hears the same audio at the same perceived moment, regardless of varying
/// network latencies.

/// A clock sync probe sent to a peer.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClockProbe {
    /// Sequence number for matching probes to responses
    pub seq: u64,
    /// Sender's monotonic timestamp in microseconds
    pub t1_us: u64,
}

/// Response to a clock probe.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClockProbeResponse {
    pub seq: u64,
    /// Original sender's timestamp (echoed back)
    pub t1_us: u64,
    /// Responder's timestamp when probe was received
    pub t2_us: u64,
    /// Responder's timestamp when response was sent
    pub t3_us: u64,
}

/// Per-peer clock synchronization state.
struct PeerClock {
    /// Estimated one-way latency to this peer in microseconds
    latency_us: i64,
    /// Estimated clock offset: peer_time - local_time in microseconds
    /// Positive means peer's clock is ahead of ours
    offset_us: i64,
    /// Exponential moving average smoothing factor
    alpha: f64,
    /// Number of successful sync rounds
    sync_count: u64,
    /// Pending probes awaiting response
    pending: HashMap<u64, Instant>,
}

impl PeerClock {
    fn new() -> Self {
        Self {
            latency_us: 0,
            offset_us: 0,
            alpha: 0.3, // Smoothing factor — converges in ~5 rounds
            sync_count: 0,
            pending: HashMap::new(),
        }
    }

    /// Process an NTP-style round-trip measurement.
    /// t1 = local send time, t2 = remote receive time, t3 = remote send time, t4 = local receive time
    fn update(&mut self, t1_us: u64, t2_us: u64, t3_us: u64, t4_us: u64) {
        // NTP algorithm:
        // Round-trip delay = (t4 - t1) - (t3 - t2)
        // Clock offset = ((t2 - t1) + (t3 - t4)) / 2

        let rtt = (t4_us as i64 - t1_us as i64) - (t3_us as i64 - t2_us as i64);
        let offset = ((t2_us as i64 - t1_us as i64) + (t3_us as i64 - t4_us as i64)) / 2;
        let one_way = rtt / 2;

        if self.sync_count == 0 {
            // First measurement — use directly
            self.latency_us = one_way;
            self.offset_us = offset;
        } else {
            // Exponential moving average for stability
            self.latency_us =
                (self.alpha * one_way as f64 + (1.0 - self.alpha) * self.latency_us as f64) as i64;
            self.offset_us =
                (self.alpha * offset as f64 + (1.0 - self.alpha) * self.offset_us as f64) as i64;
        }

        self.sync_count += 1;
    }
}

/// Manages clock synchronization across all voice peers.
pub struct AudioSync {
    /// Reference epoch — all timestamps are relative to this
    epoch: Instant,
    /// Per-peer clock state
    peers: HashMap<String, PeerClock>,
    /// Next probe sequence number
    next_seq: u64,
    /// Target alignment delay in microseconds.
    /// All streams are delayed by at least this much so they can be aligned.
    /// Set to the maximum observed one-way latency + margin.
    alignment_delay_us: i64,
    /// Minimum alignment delay (floor) in microseconds
    min_delay_us: i64,
    /// Safety margin added to max latency
    margin_us: i64,
}

impl AudioSync {
    pub fn new() -> Self {
        Self {
            epoch: Instant::now(),
            peers: HashMap::new(),
            next_seq: 0,
            alignment_delay_us: 100_000, // Start with 100ms default
            min_delay_us: 50_000,        // Never go below 50ms
            margin_us: 20_000,           // 20ms safety margin
        }
    }

    /// Get the current synchronized timestamp in microseconds since epoch.
    pub fn now_us(&self) -> u64 {
        self.epoch.elapsed().as_micros() as u64
    }

    /// Register a new peer for clock sync.
    pub fn add_peer(&mut self, peer_id: &str) {
        self.peers.entry(peer_id.to_string()).or_insert_with(PeerClock::new);
    }

    /// Remove a peer.
    pub fn remove_peer(&mut self, peer_id: &str) {
        self.peers.remove(peer_id);
        self.recalculate_alignment_delay();
    }

    /// Create a clock probe to send to a peer.
    pub fn create_probe(&mut self, peer_id: &str) -> ClockProbe {
        let seq = self.next_seq;
        self.next_seq += 1;
        let t1_us = self.now_us();

        if let Some(peer) = self.peers.get_mut(peer_id) {
            peer.pending.insert(seq, Instant::now());
        }

        ClockProbe { seq, t1_us }
    }

    /// Handle an incoming clock probe — generate a response.
    pub fn handle_probe(&self, probe: &ClockProbe) -> ClockProbeResponse {
        let now = self.now_us();
        ClockProbeResponse {
            seq: probe.seq,
            t1_us: probe.t1_us,
            t2_us: now,
            t3_us: now, // In practice t2 and t3 are very close
        }
    }

    /// Handle a clock probe response — update peer's clock estimate.
    pub fn handle_probe_response(&mut self, peer_id: &str, response: &ClockProbeResponse) {
        let t4_us = self.now_us();

        if let Some(peer) = self.peers.get_mut(peer_id) {
            peer.pending.remove(&response.seq);
            peer.update(response.t1_us, response.t2_us, response.t3_us, t4_us);

            if peer.sync_count % 5 == 0 {
                info!(
                    "Clock sync with {}: offset={}us, latency={}us ({} rounds)",
                    peer_id, peer.offset_us, peer.latency_us, peer.sync_count
                );
            }
        }

        self.recalculate_alignment_delay();
    }

    /// Calculate how long to delay a received audio packet from a specific peer
    /// so it aligns with all other streams.
    ///
    /// Returns the delay in microseconds.
    pub fn playback_delay_for_peer(&self, peer_id: &str) -> i64 {
        if let Some(peer) = self.peers.get(peer_id) {
            // This peer's audio arrives with `latency_us` delay.
            // We want all streams aligned at `alignment_delay_us` from send time.
            // So the additional buffer delay for this peer is:
            let additional = self.alignment_delay_us - peer.latency_us;
            additional.max(0)
        } else {
            self.alignment_delay_us
        }
    }

    /// Convert a remote peer's timestamp to local time, accounting for clock offset.
    pub fn remote_to_local_us(&self, peer_id: &str, remote_us: u64) -> u64 {
        if let Some(peer) = self.peers.get(peer_id) {
            // remote_time = local_time + offset
            // local_time = remote_time - offset
            (remote_us as i64 - peer.offset_us) as u64
        } else {
            remote_us
        }
    }

    /// Get the current alignment delay in milliseconds.
    pub fn alignment_delay_ms(&self) -> f64 {
        self.alignment_delay_us as f64 / 1000.0
    }

    /// Get a peer's measured one-way latency in milliseconds.
    pub fn peer_latency_ms(&self, peer_id: &str) -> f64 {
        self.peers
            .get(peer_id)
            .map(|p| p.latency_us as f64 / 1000.0)
            .unwrap_or(0.0)
    }

    /// Get a peer's clock offset in milliseconds.
    pub fn peer_offset_ms(&self, peer_id: &str) -> f64 {
        self.peers
            .get(peer_id)
            .map(|p| p.offset_us as f64 / 1000.0)
            .unwrap_or(0.0)
    }

    /// Check if a peer has completed enough sync rounds to be reliable.
    pub fn peer_is_synced(&self, peer_id: &str) -> bool {
        self.peers
            .get(peer_id)
            .map(|p| p.sync_count >= 3)
            .unwrap_or(false)
    }

    // ── Internal ────────────────────────────────────────────────────────

    /// Recalculate the alignment delay based on the worst-case latency across all peers.
    fn recalculate_alignment_delay(&mut self) {
        let max_latency = self
            .peers
            .values()
            .filter(|p| p.sync_count > 0)
            .map(|p| p.latency_us)
            .max()
            .unwrap_or(0);

        let target = max_latency + self.margin_us;
        self.alignment_delay_us = target.max(self.min_delay_us);
    }
}

/// A delay-compensated audio buffer for a single peer's incoming stream.
/// Buffers audio and releases it at the aligned playback time.
pub struct AlignedAudioBuffer {
    /// Audio samples with their target playback time
    entries: Vec<AudioEntry>,
    /// Playback delay for this peer in samples (at 48kHz)
    delay_samples: usize,
    /// Current read position
    read_pos: usize,
}

struct AudioEntry {
    /// When these samples should be played (in local synced time, microseconds)
    target_time_us: u64,
    /// Decoded PCM samples
    samples: Vec<f32>,
    /// Whether this entry has been consumed
    consumed: bool,
}

impl AlignedAudioBuffer {
    pub fn new(delay_us: i64) -> Self {
        // Convert delay from microseconds to samples at 48kHz
        let delay_samples = ((delay_us as f64 / 1_000_000.0) * 48000.0) as usize;
        Self {
            entries: Vec::new(),
            delay_samples,
            read_pos: 0,
        }
    }

    /// Push decoded audio with a target playback timestamp.
    pub fn push(&mut self, target_time_us: u64, samples: Vec<f32>) {
        self.entries.push(AudioEntry {
            target_time_us,
            samples,
            consumed: false,
        });

        // Evict old entries (>2 seconds old)
        self.entries.retain(|e| !e.consumed);
        if self.entries.len() > 200 {
            self.entries.drain(..self.entries.len() - 200);
        }
    }

    /// Read aligned samples for the current playback time.
    /// Returns silence if no samples are ready yet (still buffering).
    pub fn read(&mut self, now_us: u64, output: &mut [f32]) {
        output.fill(0.0);

        let mut write_pos = 0;
        for entry in &mut self.entries {
            if entry.consumed {
                continue;
            }
            // Check if this entry's playback time has arrived
            // (accounting for the alignment delay)
            if entry.target_time_us <= now_us {
                let remaining = entry.samples.len() - 0; // simplified
                let to_copy = remaining.min(output.len() - write_pos);
                if to_copy > 0 {
                    // Mix into output (additive for multi-peer mixing)
                    for i in 0..to_copy {
                        output[write_pos + i] += entry.samples[i];
                    }
                    write_pos += to_copy;
                }
                entry.consumed = true;
            }
        }
    }

    /// Update the delay for this buffer (e.g., when sync measurements change).
    pub fn update_delay(&mut self, delay_us: i64) {
        self.delay_samples = ((delay_us as f64 / 1_000_000.0) * 48000.0) as usize;
    }
}
