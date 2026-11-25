#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("Invalid timecode string: {0}")]
    InvalidTimecode(String),
    #[error("Item not found: {0}")]
    NotFound(String),
    #[error("Bin not found: {0}")]
    BinNotFound(String),
}

/// SMPTE Timecode representation (HH:MM:SS:FF)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SmpteTimecode {
    pub hours: u32,
    pub minutes: u32,
    pub seconds: u32,
    pub frames: u32,
    pub fps: u32,
}

impl SmpteTimecode {
    pub fn new(hours: u32, minutes: u32, seconds: u32, frames: u32, fps: u32) -> Self {
        let actual_fps = if fps == 0 { 24 } else { fps };
        Self {
            hours,
            minutes,
            seconds,
            frames: frames % actual_fps,
            fps: actual_fps,
        }
    }

    pub fn from_total_frames(total_frames: u64, fps: u32) -> Self {
        let f = if fps == 0 { 24 } else { fps } as u64;
        let frames = (total_frames % f) as u32;
        let total_seconds = total_frames / f;
        let seconds = (total_seconds % 60) as u32;
        let total_minutes = total_seconds / 60;
        let minutes = (total_minutes % 60) as u32;
        let hours = (total_minutes / 60) as u32;

        Self {
            hours,
            minutes,
            seconds,
            frames,
            fps: f as u32,
        }
    }

    pub fn to_total_frames(&self) -> u64 {
        let f = self.fps as u64;
        let s = self.seconds as u64 + (self.minutes as u64 * 60) + (self.hours as u64 * 3600);
        s * f + self.frames as u64
    }

    pub fn to_string_tc(&self) -> String {
        format!(
            "{:02}:{:02}:{:02}:{:02}",
            self.hours, self.minutes, self.seconds, self.frames
        )
    }

    pub fn parse(s: &str, fps: u32) -> Result<Self, MediaError> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 4 {
            return Err(MediaError::InvalidTimecode(s.to_string()));
        }
        let h = parts[0]
            .parse::<u32>()
            .map_err(|_| MediaError::InvalidTimecode(s.to_string()))?;
        let m = parts[1]
            .parse::<u32>()
            .map_err(|_| MediaError::InvalidTimecode(s.to_string()))?;
        let sec = parts[2]
            .parse::<u32>()
            .map_err(|_| MediaError::InvalidTimecode(s.to_string()))?;
        let f = parts[3]
            .parse::<u32>()
            .map_err(|_| MediaError::InvalidTimecode(s.to_string()))?;
        Ok(Self::new(h, m, sec, f, fps))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaKind {
    Video,
    Audio,
    ImageSequence,
    StillGraphic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VideoCodec {
    ProRes422,
    ProRes4444,
    DnxHrHq,
    H264,
    H265,
    Av1,
    RawBayer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioCodec {
    PcmLinear16,
    PcmLinear24,
    Float32,
    Aac,
    Flac,
}

/// Metadata item for a media file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaItem {
    pub id: String,
    pub name: String,
    pub file_path: String,
    pub kind: MediaKind,
    pub video_codec: Option<VideoCodec>,
    pub audio_codec: Option<AudioCodec>,
    pub resolution: [u32; 2],
    pub fps: (u32, u32),
    pub duration_frames: u64,
    pub start_timecode: SmpteTimecode,
    pub proxy_generated: bool,
    pub tags: Vec<String>,
}

impl MediaItem {
    pub fn new_video(
        id: impl Into<String>,
        name: impl Into<String>,
        file_path: impl Into<String>,
        codec: VideoCodec,
        resolution: [u32; 2],
        fps: (u32, u32),
        duration_frames: u64,
    ) -> Self {
        let fps_num = if fps.0 == 0 { 24 } else { fps.0 / fps.1.max(1) };
        Self {
            id: id.into(),
            name: name.into(),
            file_path: file_path.into(),
            kind: MediaKind::Video,
            video_codec: Some(codec),
            audio_codec: None,
            resolution,
            fps,
            duration_frames,
            start_timecode: SmpteTimecode::new(1, 0, 0, 0, fps_num),
            proxy_generated: false,
            tags: Vec::new(),
        }
    }
}

/// Bin folder organizing media assets
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaBin {
    pub id: String,
    pub name: String,
    pub item_ids: Vec<String>,
    pub sub_bin_ids: Vec<String>,
}

impl MediaBin {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            item_ids: Vec::new(),
            sub_bin_ids: Vec::new(),
        }
    }
}

/// DaVinci Resolve-class Media Pool
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MediaPool {
    pub items: HashMap<String, MediaItem>,
    pub bins: HashMap<String, MediaBin>,
    pub root_bin_id: String,
}

impl MediaPool {
    pub fn new() -> Self {
        let root_bin = MediaBin::new("root_bin", "Master");
        let mut bins = HashMap::new();
        bins.insert("root_bin".to_string(), root_bin);

        Self {
            items: HashMap::new(),
            bins,
            root_bin_id: "root_bin".to_string(),
        }
    }

    pub fn add_item(
        &mut self,
        item: MediaItem,
        bin_id: Option<&str>,
    ) -> Result<String, MediaError> {
        let id = item.id.clone();
        let target_bin = bin_id.unwrap_or(&self.root_bin_id);
        if let Some(bin) = self.bins.get_mut(target_bin) {
            bin.item_ids.push(id.clone());
        } else {
            return Err(MediaError::BinNotFound(target_bin.to_string()));
        }

        self.items.insert(id.clone(), item);
        Ok(id)
    }

    pub fn create_bin(
        &mut self,
        name: &str,
        parent_bin_id: Option<&str>,
    ) -> Result<String, MediaError> {
        let bin_id = format!("bin_{}", self.bins.len() + 1);
        let new_bin = MediaBin::new(bin_id.clone(), name);
        let parent = parent_bin_id.unwrap_or(&self.root_bin_id);

        if let Some(p) = self.bins.get_mut(parent) {
            p.sub_bin_ids.push(bin_id.clone());
        } else {
            return Err(MediaError::BinNotFound(parent.to_string()));
        }

        self.bins.insert(bin_id.clone(), new_bin);
        Ok(bin_id)
    }

    pub fn find_by_tag(&self, tag: &str) -> Vec<&MediaItem> {
        self.items
            .values()
            .filter(|i| i.tags.iter().any(|t| t == tag))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smpte_timecode_math() {
        let tc = SmpteTimecode::new(1, 0, 0, 0, 24);
        assert_eq!(tc.to_total_frames(), 86400); // 1 hr at 24 fps
        assert_eq!(tc.to_string_tc(), "01:00:00:00");

        let tc2 = SmpteTimecode::from_total_frames(86424, 24);
        assert_eq!(tc2.to_string_tc(), "01:00:01:00");

        let parsed = SmpteTimecode::parse("01:00:02:12", 24);
        assert!(parsed.is_ok());
        if let Ok(p) = parsed {
            assert_eq!(p.seconds, 2);
            assert_eq!(p.frames, 12);
        }
    }

    #[test]
    fn test_media_pool_operations() {
        let mut pool = MediaPool::new();
        let sub_bin = pool.create_bin("VFX Renders", None);
        assert!(sub_bin.is_ok());

        let clip = MediaItem::new_video(
            "hypersonic_mach10",
            "Mach 10 Flow",
            "/media/hypersonic.mov",
            VideoCodec::ProRes422,
            [3840, 2160],
            (24, 1),
            240,
        );

        let add_res = pool.add_item(clip, sub_bin.ok().as_deref());
        assert!(add_res.is_ok());
        assert_eq!(pool.items.len(), 1);
    }
}
