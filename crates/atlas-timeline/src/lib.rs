#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum TimelineError {
    #[error("Track not found: {0}")]
    TrackNotFound(String),
    #[error("Clip not found: {0}")]
    ClipNotFound(String),
    #[error("Invalid split frame {0} outside clip bounds [{1}, {2}]")]
    InvalidSplitFrame(u64, u64, u64),
    #[error("Serialization failed: {0}")]
    Serialization(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackKind {
    Video,
    Audio,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineClip {
    pub id: String,
    pub name: String,
    pub media_id: String,
    pub track_id: String,
    pub source_in: u64,
    pub source_out: u64,
    pub timeline_in: u64,
    pub timeline_out: u64,
    pub speed: f64,
    pub enabled: bool,
}

impl TimelineClip {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        media_id: impl Into<String>,
        track_id: impl Into<String>,
        source_in: u64,
        duration: u64,
        timeline_in: u64,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            media_id: media_id.into(),
            track_id: track_id.into(),
            source_in,
            source_out: source_in + duration,
            timeline_in,
            timeline_out: timeline_in + duration,
            speed: 1.0,
            enabled: true,
        }
    }

    pub fn duration(&self) -> u64 {
        self.timeline_out.saturating_sub(self.timeline_in)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransitionKind {
    CrossDissolve,
    DipToBlack,
    DipToWhite,
    WipeLeft,
    WipeRight,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transition {
    pub id: String,
    pub kind: TransitionKind,
    pub duration_frames: u64,
    pub leading_clip_id: String,
    pub trailing_clip_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineTrack {
    pub id: String,
    pub name: String,
    pub kind: TrackKind,
    pub clips: Vec<TimelineClip>,
    pub muted: bool,
    pub solo: bool,
    pub locked: bool,
}

impl TimelineTrack {
    pub fn new(id: impl Into<String>, name: impl Into<String>, kind: TrackKind) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            kind,
            clips: Vec::new(),
            muted: false,
            solo: false,
            locked: false,
        }
    }
}

/// DaVinci Resolve-class Multi-Track Timeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timeline {
    pub id: String,
    pub name: String,
    pub fps: u32,
    pub start_timecode_frames: u64,
    pub tracks: Vec<TimelineTrack>,
    pub transitions: Vec<Transition>,
}

impl Timeline {
    pub fn new(id: impl Into<String>, name: impl Into<String>, fps: u32) -> Self {
        let f = if fps == 0 { 24 } else { fps };
        Self {
            id: id.into(),
            name: name.into(),
            fps: f,
            start_timecode_frames: 3600 * (f as u64), // Default 01:00:00:00
            tracks: Vec::new(),
            transitions: Vec::new(),
        }
    }

    pub fn add_track(&mut self, name: &str, kind: TrackKind) -> String {
        let track_id = format!("track_{}_{}", match kind {
            TrackKind::Video => "v",
            TrackKind::Audio => "a",
        }, self.tracks.len() + 1);
        self.tracks.push(TimelineTrack::new(track_id.clone(), name, kind));
        track_id
    }

    pub fn add_clip_to_track(
        &mut self,
        track_id: &str,
        clip: TimelineClip,
    ) -> Result<(), TimelineError> {
        if let Some(track) = self.tracks.iter_mut().find(|t| t.id == track_id) {
            track.clips.push(clip);
            track.clips.sort_by_key(|c| c.timeline_in);
            Ok(())
        } else {
            Err(TimelineError::TrackNotFound(track_id.to_string()))
        }
    }

    /// Split a clip at a given timeline frame into two adjacent clips
    pub fn split_clip(&mut self, clip_id: &str, split_timeline_frame: u64) -> Result<(String, String), TimelineError> {
        for track in &mut self.tracks {
            if let Some(pos) = track.clips.iter().position(|c| c.id == clip_id) {
                let orig = &track.clips[pos];
                if split_timeline_frame <= orig.timeline_in || split_timeline_frame >= orig.timeline_out {
                    return Err(TimelineError::InvalidSplitFrame(
                        split_timeline_frame,
                        orig.timeline_in,
                        orig.timeline_out,
                    ));
                }

                let offset = split_timeline_frame - orig.timeline_in;
                let clip1_id = format!("{}_part1", orig.id);
                let clip2_id = format!("{}_part2", orig.id);

                let part1 = TimelineClip {
                    id: clip1_id.clone(),
                    name: format!("{} (Part 1)", orig.name),
                    media_id: orig.media_id.clone(),
                    track_id: track.id.clone(),
                    source_in: orig.source_in,
                    source_out: orig.source_in + offset,
                    timeline_in: orig.timeline_in,
                    timeline_out: split_timeline_frame,
                    speed: orig.speed,
                    enabled: orig.enabled,
                };

                let part2 = TimelineClip {
                    id: clip2_id.clone(),
                    name: format!("{} (Part 2)", orig.name),
                    media_id: orig.media_id.clone(),
                    track_id: track.id.clone(),
                    source_in: orig.source_in + offset,
                    source_out: orig.source_out,
                    timeline_in: split_timeline_frame,
                    timeline_out: orig.timeline_out,
                    speed: orig.speed,
                    enabled: orig.enabled,
                };

                track.clips.remove(pos);
                track.clips.insert(pos, part2);
                track.clips.insert(pos, part1);

                return Ok((clip1_id, clip2_id));
            }
        }
        Err(TimelineError::ClipNotFound(clip_id.to_string()))
    }

    /// Export to standard OpenTimelineIO (.otio) JSON schema
    pub fn to_otio_json(&self) -> Result<String, TimelineError> {
        let mut otio_tracks = Vec::new();

        for track in &self.tracks {
            let mut otio_children = Vec::new();
            for clip in &track.clips {
                let otio_clip = serde_json::json!({
                    "OTIO_SCHEMA": "Clip.1",
                    "name": clip.name,
                    "source_range": {
                        "OTIO_SCHEMA": "TimeRange.1",
                        "start_time": {
                            "OTIO_SCHEMA": "RationalTime.1",
                            "rate": self.fps,
                            "value": clip.source_in
                        },
                        "duration": {
                            "OTIO_SCHEMA": "RationalTime.1",
                            "rate": self.fps,
                            "value": clip.duration()
                        }
                    },
                    "media_reference": {
                        "OTIO_SCHEMA": "ExternalReference.1",
                        "target_url": clip.media_id
                    }
                });
                otio_children.push(otio_clip);
            }

            let kind_str = match track.kind {
                TrackKind::Video => "Video",
                TrackKind::Audio => "Audio",
            };

            otio_tracks.push(serde_json::json!({
                "OTIO_SCHEMA": "Track.1",
                "name": track.name,
                "kind": kind_str,
                "children": otio_children
            }));
        }

        let otio_doc = serde_json::json!({
            "OTIO_SCHEMA": "Timeline.1",
            "name": self.name,
            "global_start_time": {
                "OTIO_SCHEMA": "RationalTime.1",
                "rate": self.fps,
                "value": self.start_timecode_frames
            },
            "tracks": {
                "OTIO_SCHEMA": "Stack.1",
                "children": otio_tracks
            }
        });

        serde_json::to_string_pretty(&otio_doc).map_err(|e| TimelineError::Serialization(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeline_clip_split_and_otio() {
        let mut timeline = Timeline::new("tl_01", "Hypersonic Aircraft Commercial", 24);
        let v1_id = timeline.add_track("Video 1", TrackKind::Video);

        let clip = TimelineClip::new(
            "clip_scramjet",
            "Scramjet Ignition",
            "media_scramjet_4k",
            &v1_id,
            0,
            100,
            0,
        );

        let add_res = timeline.add_clip_to_track(&v1_id, clip);
        assert!(add_res.is_ok());

        let split_res = timeline.split_clip("clip_scramjet", 40);
        assert!(split_res.is_ok());

        if let Some(track) = timeline.tracks.first() {
            assert_eq!(track.clips.len(), 2);
            assert_eq!(track.clips[0].duration(), 40);
            assert_eq!(track.clips[1].duration(), 60);
        }

        let otio = timeline.to_otio_json();
        assert!(otio.is_ok());
        if let Ok(json_str) = otio {
            assert!(json_str.contains("OTIO_SCHEMA"));
            assert!(json_str.contains("Scramjet Ignition (Part 1)"));
        }
    }
}
