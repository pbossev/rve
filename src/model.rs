//! Application state models, configuration constants, and data structures.

use crate::{
    audio::AudioPlayer,
    ffmpeg::{self, FrameIterator},
    view,
};
use std::error::Error;

/// Number of frames tracked for calculating moving average FPS.
pub const NUM_FRAMES_TO_TRACK_FPS: u8 = 10;
/// Total row height reserved for the UI interface below the video canvas.
pub const UI_HEIGHT: u16 = 8;
/// Relative row offset for the timeline bar within the UI.
pub const TIMELINE_ROW: u16 = 2;
/// Maximum pixel width for high-resolution graphics protocol rendering.
pub const MAX_HR_WIDTH: u32 = 1280;

/// Video display resolution mode.
#[derive(Clone, Copy, PartialEq)]
pub enum DisplayMode {
    /// Low-resolution half-block character rendering.
    LowResBlock,
    /// High-resolution pixel graphics protocol rendering (e.g. Kitty).
    HighResPixel,
}

/// Active selection target type on the timeline.
pub enum HoverMode {
    /// Targeting a split marker.
    Markers,
    /// Targeting a video segment between markers.
    Segments,
}

/// Current hover target and position on the timeline.
pub struct Hovering {
    /// Target mode (marker or segment).
    pub mode: HoverMode,
    /// Zero-based index of the hovered marker or segment.
    pub position: usize,
}

/// Terminal character cell color cache for differential rendering.
#[derive(Clone, Default)]
pub struct TerminalState {
    /// Terminal width in columns.
    pub width: u16,
    /// Terminal height in rows.
    pub height: u16,
    /// Previous frame (foreground RGB, background RGB) per character cell.
    pub blocks: Vec<(image::Rgb<u8>, image::Rgb<u8>)>,
}

/// Central application state holding video frames, timeline markers, and playback status.
pub struct Model {
    /// Width of the terminal window in character columns.
    pub terminal_cols: u16,
    /// Height of the terminal window in character rows.
    pub terminal_rows: u16,
    /// Extracted metadata of the input video file.
    pub video_metadata: VideoMetadata,
    /// Stream iterator producing video frames from FFmpeg.
    pub frame_iterator: FrameIterator,
    /// Audio playback engine.
    pub audio_player: AudioPlayer,
    /// Currently displayed video frame image.
    pub current_frame: Option<image::RgbImage>,
    /// Current zero-based frame index.
    pub frame_number: u32,
    /// Frame index from the previous render cycle.
    pub prev_frame_number: u32,
    /// Whether video playback is currently paused.
    pub paused: bool,
    /// Timestamps in seconds where markers are placed.
    pub markers: Vec<f64>,
    /// Inclusion flags for each timeline segment between markers.
    pub segments_included: Vec<bool>,
    /// Currently hovered timeline item.
    pub hovered_item: Hovering,
    /// Whether shortcut key legends are hidden.
    pub hide_controls: bool,
    /// Whether a full screen clear is required on next frame.
    pub needs_to_clear: bool,
    /// Instant of the last frame update.
    pub prev_instant: std::time::Instant,
    /// Time accumulator for frame rate synchronization.
    pub accumulated_time: f64,
    /// Calculated recent rendering frame rate in FPS.
    pub recent_fps: Option<f64>,
    /// Instant of the last FPS update check.
    pub last_fps_check: std::time::Instant,
    /// Whether to concatenate exported segments into a single file.
    pub single_output: bool,
    /// Current display rendering mode.
    pub display_mode: DisplayMode,
    /// Whether the terminal supports high-resolution graphics.
    pub high_res_available: bool,
    /// Whether the quit confirmation prompt is currently active.
    pub exit_prompt: bool,
    /// Flag indicating an export operation has been triggered.
    pub is_saving: bool,
    /// Flag indicating the application should exit the main loop.
    pub should_exit: bool,
    /// Terminal differential buffer cache.
    pub terminal_state: TerminalState,
}

/// Basic metadata properties of the input video file.
#[derive(Clone)]
pub struct VideoMetadata {
    /// Native pixel width.
    pub width: i32,
    /// Native pixel height.
    pub height: i32,
    /// Frames per second.
    pub fps: f64,
    /// Total duration in seconds.
    pub duration_secs: f64,
    /// Duration of a single frame in seconds.
    pub seconds_per_frame: f64,
}

impl Model {
    /// Creates and initializes a new application `Model` instance.
    pub fn new(
        video_path: String,
        cols: u16,
        rows: u16,
        initial_mode: DisplayMode,
        single_output: bool,
        high_res_available: bool,
    ) -> Result<Self, Box<dyn Error>> {
        let meta = ffmpeg::get_ffprobe_video_metadata(&video_path)?;
        let (render_w, render_h) = view::calculate_render_size(
            cols,
            rows,
            meta.width as f64 / meta.height as f64,
            &meta,
            initial_mode,
        );

        let iter = FrameIterator::new(video_path, render_w, render_h)?;
        let mut audio_player = AudioPlayer::new();
        audio_player.seek(&iter.video_path, 0.0, true);

        Ok(Model {
            paused: true,
            frame_number: 0,
            prev_frame_number: 0,
            markers: Vec::new(),
            segments_included: vec![true],
            hovered_item: Hovering {
                mode: HoverMode::Segments,
                position: 0,
            },
            terminal_cols: cols,
            terminal_rows: rows,
            video_metadata: meta,
            frame_iterator: iter,
            audio_player,
            current_frame: None,
            hide_controls: false,
            needs_to_clear: true,
            prev_instant: std::time::Instant::now(),
            last_fps_check: std::time::Instant::now(),
            recent_fps: None,
            accumulated_time: 0.0,
            single_output,
            display_mode: initial_mode,
            high_res_available,
            exit_prompt: false,
            is_saving: false,
            should_exit: false,
            terminal_state: TerminalState::default(),
        })
    }

    /// Seeks the video frame iterator and audio engine to the specified timestamp in seconds.
    pub fn seek_to(&mut self, ts: f64) {
        self.current_frame = self.frame_iterator.goto(ts).ok();
        self.audio_player.seek(&self.frame_iterator.video_path, ts, self.paused);
        self.prev_instant = std::time::Instant::now();
        self.accumulated_time = 0.0;
    }
}
