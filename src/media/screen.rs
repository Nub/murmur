use anyhow::{anyhow, Result};
use std::sync::Arc;
use tracing::info;
use webrtc::api::media_engine::MIME_TYPE_VP8;
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;
use webrtc::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;

/// Screen capture and sharing via WebRTC.
/// Currently a stub — platform-specific capture backends (X11 XShm, DXGI, etc.)
/// will be integrated directly without heavy dependency chains.
pub struct ScreenShare {
    video_track: Option<Arc<TrackLocalStaticRTP>>,
    active: bool,
    width: u32,
    height: u32,
}

impl ScreenShare {
    pub fn new() -> Self {
        Self {
            video_track: None,
            active: false,
            width: 0,
            height: 0,
        }
    }

    /// Start screen capture. Returns the video track to add to peer connections.
    pub fn start(&mut self) -> Result<Arc<TrackLocalStaticRTP>> {
        self.width = 1920;
        self.height = 1080;
        info!("Screen share started ({}x{})", self.width, self.height);

        let video_track = Arc::new(TrackLocalStaticRTP::new(
            RTCRtpCodecCapability {
                mime_type: MIME_TYPE_VP8.to_owned(),
                ..Default::default()
            },
            "video".to_string(),
            "murmur-screen".to_string(),
        ));

        self.video_track = Some(video_track.clone());
        self.active = true;
        Ok(video_track)
    }

    pub fn stop(&mut self) {
        self.video_track = None;
        self.active = false;
        info!("Screen share stopped");
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn video_track(&self) -> Option<Arc<TrackLocalStaticRTP>> {
        self.video_track.clone()
    }
}
