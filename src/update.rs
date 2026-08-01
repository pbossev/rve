//! Event handling and application state update logic.

use crate::model::{DisplayMode, HoverMode, Hovering, Model, NUM_FRAMES_TO_TRACK_FPS};
use crate::view::calculate_render_size;
use crossterm::event::{Event, KeyCode, KeyModifiers};

/// Processes keyboard and terminal resize events to update application state.
pub fn update(m: &mut Model, evt: Event) -> Result<bool, String> {
    m.prev_frame_number = m.frame_number;
    let mut redraw_needed = false;

    match evt {
        Event::Key(k) => {
            if m.exit_prompt {
                match k.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        m.should_exit = true;
                        return Ok(true);
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                        m.exit_prompt = false;
                        m.needs_to_clear = true;
                        return Ok(true);
                    }
                    _ => return Ok(false),
                }
            }

            // exit keys
            if (k.modifiers == KeyModifiers::CONTROL && k.code == KeyCode::Char('c'))
                || k.code == KeyCode::Esc
                || k.code == KeyCode::Char('q')
            {
                m.exit_prompt = true;
                m.needs_to_clear = true;
                m.paused = true;
                m.audio_player.pause();
                return Ok(true);
            }

            // view update on any keypress
            if k.code != KeyCode::Null {
                redraw_needed = true;
            }

            match k.code {
                KeyCode::Char(' ') => {
                    m.paused = !m.paused;
                    if m.paused {
                        m.audio_player.pause();
                    } else {
                        m.hovered_item.mode = HoverMode::Segments;
                        let ts = m.frame_number as f64 / m.video_metadata.fps;
                        let seg = m.segment_at_ts(ts);
                        if !m.is_segment_included(seg) {
                            if let Some((_next_seg, next_ts)) = m.find_next_included_segment_start(seg) {
                                m.seek_to(next_ts);
                            } else {
                                m.paused = true;
                                m.audio_player.pause();
                            }
                        } else {
                            m.seek_to(ts);
                        }
                    }
                }
                KeyCode::Char('+') | KeyCode::Char('=') => {
                    m.audio_player.set_volume(m.audio_player.volume + 0.1);
                }
                KeyCode::Char('-') | KeyCode::Char('_') => {
                    m.audio_player.set_volume(m.audio_player.volume - 0.1);
                }
                KeyCode::Char('<') | KeyCode::Char(',') if k.modifiers.contains(KeyModifiers::SHIFT) => {
                    adjust_speed(m, false);
                }
                KeyCode::Char('>') | KeyCode::Char('.') if k.modifiers.contains(KeyModifiers::SHIFT) => {
                    adjust_speed(m, true);
                }
                KeyCode::Char('<') => adjust_speed(m, false),
                KeyCode::Char('>') => adjust_speed(m, true),
                KeyCode::Char('a') => {
                    m.audio_player.set_export_volume(m.audio_player.export_volume + 0.1);
                }
                KeyCode::Char('A') => {
                    m.audio_player.set_export_volume(m.audio_player.export_volume - 0.1);
                }
                KeyCode::Char('?') => {
                    m.hide_controls = !m.hide_controls;
                }
                KeyCode::Char('r') => {
                    if m.high_res_available {
                        m.display_mode = match m.display_mode {
                            DisplayMode::LowResBlock => DisplayMode::HighResPixel,
                            DisplayMode::HighResPixel => DisplayMode::LowResBlock,
                        };
                        let video_aspect =
                            m.video_metadata.width as f64 / m.video_metadata.height as f64;
                        let (w, h) = calculate_render_size(
                            m.terminal_cols,
                            m.terminal_rows,
                            video_aspect,
                            &m.video_metadata,
                            m.display_mode,
                        );
                        m.frame_iterator.resize(w, h);
                        m.needs_to_clear = true;
                    }
                }
                KeyCode::Char('v') => toggle_marker(m),
                KeyCode::Char('t') => toggle_segment(m),
                KeyCode::Char('i') => {
                    m.single_output = !m.single_output;
                    m.needs_to_clear = true;
                }
                KeyCode::Char('s') => {
                    m.is_saving = true;
                    return Ok(true);
                }
                KeyCode::Char('[') => nav_marker_prev(m),
                KeyCode::Char(']') => nav_marker_next(m),
                KeyCode::Char('.') => {
                    if m.paused {
                        advance(m, 1);
                    }
                }
                KeyCode::Char(',') => {
                    if m.paused {
                        advance(m, -1);
                    }
                }
                KeyCode::Char(c @ '0'..='9') => skip_to(m, (c as u32 - '0' as u32) * 10),

                KeyCode::Left => {
                    if k.modifiers.contains(KeyModifiers::CONTROL) {
                        seek_by_seconds(m, -60.0);
                    } else if k.modifiers.contains(KeyModifiers::ALT) {
                        seek_by_seconds(m, -30.0);
                    } else {
                        seek_by_seconds(m, -5.0);
                    }
                }
                KeyCode::Right => {
                    if k.modifiers.contains(KeyModifiers::CONTROL) {
                        seek_by_seconds(m, 60.0);
                    } else if k.modifiers.contains(KeyModifiers::ALT) {
                        seek_by_seconds(m, 30.0);
                    } else {
                        seek_by_seconds(m, 5.0);
                    }
                }
                _ => redraw_needed = false,
            }
        }
        Event::Resize(c, r) => {
            m.terminal_cols = c;
            m.terminal_rows = r;
            m.needs_to_clear = true;

            let video_aspect = m.video_metadata.width as f64 / m.video_metadata.height as f64;
            let (w, h) =
                calculate_render_size(c, r, video_aspect, &m.video_metadata, m.display_mode);
            m.frame_iterator.resize(w, h);
            let ts = m.frame_number as f64 / m.video_metadata.fps;
            m.seek_to(ts);
            redraw_needed = true;
        }
        _ => redraw_needed = false,
    }

    if m.current_frame.is_none() {
        m.current_frame = m.frame_iterator.take_frame().ok();
        redraw_needed = true;
    }

    if m.paused {
        m.prev_instant = std::time::Instant::now();
        m.accumulated_time = 0.0;
        return Ok(redraw_needed);
    }

    let frames = calc_frames(m);
    if frames > 0 {
        let target_frame = m.frame_number + frames;
        let target_ts = target_frame as f64 * m.video_metadata.seconds_per_frame;
        let current_seg = m.segment_at_ts(target_ts);

        if !m.is_segment_included(current_seg) {
            if let Some((_next_seg, next_ts)) = m.find_next_included_segment_start(current_seg) {
                m.seek_to(next_ts);
                redraw_needed = true;
            } else {
                m.paused = true;
                m.audio_player.pause();
                redraw_needed = true;
            }
        } else {
            if let Ok(frame) = m.frame_iterator.skip_frames(frames) {
                m.current_frame = Some(frame);
                m.frame_number = target_frame;
                m.hovered_item.position = current_seg;
                redraw_needed = true;
            } else {
                m.paused = true;
                m.audio_player.pause();
                redraw_needed = true;
            }
        }
    }

    let now = std::time::Instant::now();
    if m.frame_iterator.num_frames_rendered > 0
        && m.frame_iterator.num_frames_rendered % NUM_FRAMES_TO_TRACK_FPS as u32 == 0
    {
        let dt = (now - m.last_fps_check).as_secs_f64();
        m.recent_fps = Some(NUM_FRAMES_TO_TRACK_FPS as f64 / dt);
        m.last_fps_check = now;
    }
    m.prev_instant = now;

    Ok(redraw_needed)
}

/// Calculates frame advancement count based on real time elapsed, speed scaling, and frame rate drift.
fn calc_frames(m: &mut Model) -> u32 {
    let dt = (std::time::Instant::now() - m.prev_instant).as_secs_f64();
    let effective_fps = m.video_metadata.fps * m.speed;
    let effective_spf = m.video_metadata.seconds_per_frame / m.speed;
    let mut n = (dt * effective_fps).floor() as u32;
    // account for frame-rate drift by tracking error
    let err = dt - (n as f64 * effective_spf);
    m.accumulated_time += err;
    if m.accumulated_time > effective_spf {
        n += 1;
        m.accumulated_time -= effective_spf;
    }
    n
}

/// Cycles playback speed up or down through standard speed presets.
fn adjust_speed(m: &mut Model, increase: bool) {
    let speeds = [0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 2.0];
    let current_speed = m.speed;
    let idx = speeds
        .iter()
        .position(|&s| (s - current_speed).abs() < 0.01)
        .unwrap_or(3);
    let new_idx = if increase {
        (idx + 1).min(speeds.len() - 1)
    } else {
        idx.saturating_sub(1)
    };
    m.speed = speeds[new_idx];
    m.audio_player.set_speed(m.speed as f32);
    if !m.paused {
        let ts = m.frame_number as f64 / m.video_metadata.fps;
        m.seek_to(ts);
    }
}

/// Advances timeline segment hover position forward as playhead moves.
fn update_segment_fwd(m: &mut Model) {
    let ts = m.frame_number as f64 * m.video_metadata.seconds_per_frame;
    while let Some(marker_ts) = m.markers.get(m.hovered_item.position) {
        if ts > *marker_ts {
            m.hovered_item.position += 1;
        } else {
            break;
        }
    }
}

/// Rewinds timeline segment hover position backward as playhead seeks back.
fn update_segment_back(m: &mut Model) {
    let ts = m.frame_number as f64 * m.video_metadata.seconds_per_frame;
    while m.hovered_item.position > 0 {
        if let Some(marker_ts) = m.markers.get(m.hovered_item.position - 1) {
            if ts < *marker_ts {
                m.hovered_item.position -= 1;
            } else {
                break;
            }
        }
    }
}

/// Seeks playhead by a relative duration in seconds.
fn seek_by_seconds(m: &mut Model, seconds: f64) {
    let max_frame = (m.video_metadata.duration_secs * m.video_metadata.fps).round() as u32;
    let frames_to_seek = (seconds * m.video_metadata.fps).round() as i32;

    // calc new frame number, make sure under max
    let new_frame_num = (m.frame_number as i32 + frames_to_seek).max(0) as u32;
    m.frame_number = new_frame_num.min(max_frame);

    // go to the new timestamp
    let ts = m.frame_number as f64 / m.video_metadata.fps;
    m.seek_to(ts);

    // update segment hover state
    if frames_to_seek > 0 {
        update_segment_fwd(m);
    } else {
        update_segment_back(m);
    }
    m.hovered_item.mode = HoverMode::Segments;
}

/// Navigates playhead to the previous timeline marker or video start.
fn nav_marker_prev(m: &mut Model) {
    // if at first segment (before first marker), and we navigate back, go to frame 0.
    if m.hovered_item.position == 0 {
        m.frame_number = 0;
        m.paused = true;
        m.seek_to(0.0);

        m.hovered_item = Hovering {
            mode: HoverMode::Segments,
            position: 0,
        };
        return;
    }

    // move to the marker immediately before the current segment
    let pos = m.hovered_item.position.saturating_sub(1);

    m.hovered_item = Hovering {
        mode: HoverMode::Markers,
        position: pos,
    };
    let ts = m.markers[pos];
    m.frame_number = (ts * m.video_metadata.fps) as u32;
    m.paused = true;
    m.seek_to(ts);
}

/// Navigates playhead to the next timeline marker or video end.
fn nav_marker_next(m: &mut Model) {
    let num_markers = m.markers.len();
    let current_pos = m.hovered_item.position;

    // next marker is the one that STARTS the next segment.
    let target_index = match m.hovered_item.mode {
        HoverMode::Markers => current_pos.saturating_add(1),
        HoverMode::Segments => current_pos,
    };

    if target_index >= num_markers {
        // if in the last segment, jump to end.
        let ts = m.video_metadata.duration_secs;
        m.frame_number = (ts * m.video_metadata.fps) as u32;
        m.paused = true;
        m.seek_to(ts);

        m.hovered_item.position = num_markers;
        m.hovered_item.mode = HoverMode::Segments;
        return;
    }

    m.hovered_item = Hovering {
        mode: HoverMode::Markers,
        position: target_index,
    };
    let ts = m.markers[target_index];
    m.frame_number = (ts * m.video_metadata.fps) as u32;
    m.paused = true;
    m.seek_to(ts);
}

/// Toggles split marker at current frame (creates marker or deletes existing near marker).
fn toggle_marker(m: &mut Model) {
    let ts = m.frame_number as f64 / m.video_metadata.fps;
    // tolerance for marker proximity: half a frame duration
    let tolerance = m.video_metadata.seconds_per_frame / 2.0;

    if let Some(pos) = m.markers.iter().position(|&t| (t - ts).abs() < tolerance) {
        // remove marker near the current frame
        m.markers.remove(pos);

        // when M_pos is deleted, segments pos and pos+1 merge into segment pos.
        // we remove the state of the *second* segment involved in the merge, which is at index pos + 1.
        if pos + 1 < m.segments_included.len() {
            m.segments_included.remove(pos + 1);
        }

        // switch back to segment mode, hovering the newly merged segment
        m.hovered_item = Hovering {
            mode: HoverMode::Segments,
            position: pos,
        };
    } else {
        // create market
        let pos = m
            .markers
            .binary_search_by(|t| t.partial_cmp(&ts).unwrap())
            .unwrap_or_else(|p| p);

        m.markers.insert(pos, ts);

        let original_status = m.segments_included.get(pos).copied().unwrap_or(true);
        m.segments_included.insert(pos, original_status);

        update_segment_fwd(m);
        m.hovered_item.mode = HoverMode::Segments;
    }
    m.needs_to_clear = true; // redraw timeline bar
}

/// Toggles inclusion/exclusion status of the currently hovered segment.
fn toggle_segment(m: &mut Model) {
    m.hovered_item.mode = HoverMode::Segments; // force segment mode for clarity

    let seg_idx = m.hovered_item.position;
    if seg_idx < m.segments_included.len() {
        m.segments_included[seg_idx] = !m.segments_included[seg_idx];
        m.needs_to_clear = true; // redraw timeline bar

        if !m.paused && !m.segments_included[seg_idx] {
            let current_ts = m.frame_number as f64 / m.video_metadata.fps;
            if m.segment_at_ts(current_ts) == seg_idx {
                if let Some((_next_seg, next_ts)) = m.find_next_included_segment_start(seg_idx) {
                    m.seek_to(next_ts);
                } else {
                    m.paused = true;
                    m.audio_player.pause();
                }
            }
        }
    }
}

/// Advances or rewinds video playback by a single frame.
fn advance(m: &mut Model, direction: i32) {
    let max_frame = (m.video_metadata.duration_secs * m.video_metadata.fps).round() as u32;

    if direction > 0 {
        if let Ok(frame) = m.frame_iterator.take_frame() {
            m.current_frame = Some(frame);
            m.frame_number = (m.frame_number + 1).min(max_frame);
            update_segment_fwd(m);
            m.hovered_item.mode = HoverMode::Segments;
        }
    } else if direction < 0 {
        m.frame_number = m.frame_number.saturating_sub(1);
        let ts = m.frame_number as f64 / m.video_metadata.fps;
        m.seek_to(ts);

        update_segment_back(m);
        m.hovered_item.mode = HoverMode::Segments;
    }
}

/// Seeks playhead to a percentage of total video duration.
fn skip_to(m: &mut Model, pct: u32) {
    let ts = m.video_metadata.duration_secs * pct as f64 / 100.0;
    let old = m.frame_number;
    m.frame_number = (ts * m.video_metadata.fps) as u32;
    m.seek_to(ts);

    if m.frame_number > old {
        update_segment_fwd(m);
    } else {
        update_segment_back(m);
    }
    m.hovered_item.mode = HoverMode::Segments;
}
