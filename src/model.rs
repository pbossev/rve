use crate::{
    ffmpeg::{self, FrameIterator},
    view,
};
use std::error::Error;

pub const NUM_FRAMES_TO_TRACK_FPS: u8 = 10;
pub const UI_HEIGHT: u16 = 7;
pub const TIMELINE_ROW: u16 = 2;
pub const MAX_HR_WIDTH: u32 = 1280;

#[derive(Clone, Copy, PartialEq)]
pub enum DisplayMode {
    LowResBlock,
    HighResPixel,
}

pub enum HoverMode {
    Markers,
    Segments,
}

pub struct Hovering {
    pub mode: HoverMode,
    pub position: usize,
}

#[derive(Clone, Default)]
pub struct TerminalState {
    pub width: u16,
    pub height: u16,
    pub blocks: Vec<(image::Rgb<u8>, image::Rgb<u8>)>,
}

pub struct Model {
    pub terminal_cols: u16,
    pub terminal_rows: u16,
    pub video_metadata: VideoMetadata,
    pub frame_iterator: FrameIterator,
    pub current_frame: Option<image::RgbImage>,
    pub frame_number: u32,
    pub prev_frame_number: u32,
    pub paused: bool,
    pub markers: Vec<f64>,
    pub segments_included: Vec<bool>,
    pub hovered_item: Hovering,
    pub hide_controls: bool,
    pub needs_to_clear: bool,
    pub prev_instant: std::time::Instant,
    pub accumulated_time: f64,
    pub recent_fps: Option<f64>,
    pub last_fps_check: std::time::Instant,
    pub single_output: bool,
    pub display_mode: DisplayMode,
    pub high_res_available: bool,
    pub exit_prompt: bool,
    pub is_saving: bool,
    pub should_exit: bool,
    pub terminal_state: TerminalState,
}

#[derive(Clone)]
pub struct VideoMetadata {
    pub width: i32,
    pub height: i32,
    pub fps: f64,
    pub duration_secs: f64,
    pub seconds_per_frame: f64,
}

impl Model {
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
}
