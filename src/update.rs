use crate::model::{DisplayMode, HoverMode, Hovering, Model, NUM_FRAMES_TO_TRACK_FPS};
use crate::view::calculate_render_size;
use crossterm::event::{Event, KeyCode, KeyModifiers};

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
                return Ok(true);
            }

            // view update on any keypress
            if k.code != KeyCode::Null {
                redraw_needed = true;
            }

            match k.code {
                KeyCode::Char(' ') => {
                    m.paused = !m.paused;
                    if !m.paused {
                        m.hovered_item.mode = HoverMode::Segments;
                    }
                }
                KeyCode::Char('?') => {
                    m.hide_controls = !m.hide_controls;
                    m.needs_to_clear = true;
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
            m.current_frame = m.frame_iterator.goto(ts).ok();

            m.prev_instant = std::time::Instant::now();
            m.accumulated_time = 0.0;
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
        if let Ok(frame) = m.frame_iterator.skip_frames(frames) {
            m.current_frame = Some(frame);
            m.frame_number += frames;
            update_segment_fwd(m);
            redraw_needed = true;
        } else {
            m.paused = true;
            redraw_needed = true;
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

fn calc_frames(m: &mut Model) -> u32 {
    let dt = (std::time::Instant::now() - m.prev_instant).as_secs_f64();
    let mut n = (dt * m.video_metadata.fps).floor() as u32;
    // account for frame-rate drift by tracking error
    let err = dt - (n as f64 * m.video_metadata.seconds_per_frame);
    m.accumulated_time += err;
    if m.accumulated_time > m.video_metadata.seconds_per_frame {
        n += 1;
        m.accumulated_time -= m.video_metadata.seconds_per_frame;
    }
    n
}

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

fn seek_by_seconds(m: &mut Model, seconds: f64) {
    let max_frame = (m.video_metadata.duration_secs * m.video_metadata.fps).round() as u32;
    let frames_to_seek = (seconds * m.video_metadata.fps).round() as i32;

    // calc new frame number, make sure under max
    let new_frame_num = (m.frame_number as i32 + frames_to_seek).max(0) as u32;
    m.frame_number = new_frame_num.min(max_frame);

    // go to the new timestamp
    let ts = m.frame_number as f64 / m.video_metadata.fps;
    m.current_frame = m.frame_iterator.goto(ts).ok();

    m.prev_instant = std::time::Instant::now();
    m.accumulated_time = 0.0;

    // update segment hover state
    if frames_to_seek > 0 {
        update_segment_fwd(m);
    } else {
        update_segment_back(m);
    }
    m.hovered_item.mode = HoverMode::Segments;
}

fn nav_marker_prev(m: &mut Model) {
    // if at first segment (before first marker), and we navigate back, go to frame 0.
    if m.hovered_item.position == 0 {
        m.frame_number = 0;
        let ts = 0.0;
        m.current_frame = m.frame_iterator.goto(ts).ok();

        m.prev_instant = std::time::Instant::now();
        m.accumulated_time = 0.0;

        m.paused = true;
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
    m.current_frame = m.frame_iterator.goto(ts).ok();

    m.prev_instant = std::time::Instant::now();
    m.accumulated_time = 0.0;

    m.paused = true;
}

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
        m.current_frame = m.frame_iterator.goto(ts).ok();

        m.prev_instant = std::time::Instant::now();
        m.accumulated_time = 0.0;

        m.paused = true;
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
    m.current_frame = m.frame_iterator.goto(ts).ok();

    m.prev_instant = std::time::Instant::now();
    m.accumulated_time = 0.0;

    m.paused = true;
}

/// Key 'v' to toggle marker: create or delete.
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

/// Key 't' to toggle segment inclusion status
fn toggle_segment(m: &mut Model) {
    m.hovered_item.mode = HoverMode::Segments; // force segment mode for clarity

    let seg_idx = m.hovered_item.position;
    if seg_idx < m.segments_included.len() {
        m.segments_included[seg_idx] = !m.segments_included[seg_idx];
        m.needs_to_clear = true; // redraw timeline bar
    }
}

/// Moves forward or backward by a single frame.
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
        m.current_frame = m.frame_iterator.goto(ts).ok();

        m.prev_instant = std::time::Instant::now();
        m.accumulated_time = 0.0;

        update_segment_back(m);
        m.hovered_item.mode = HoverMode::Segments;
    }
}

fn skip_to(m: &mut Model, pct: u32) {
    let ts = m.video_metadata.duration_secs * pct as f64 / 100.0;
    let old = m.frame_number;
    m.frame_number = (ts * m.video_metadata.fps) as u32;
    m.current_frame = m.frame_iterator.goto(ts).ok();

    m.prev_instant = std::time::Instant::now();
    m.accumulated_time = 0.0;

    if m.frame_number > old {
        update_segment_fwd(m);
    } else {
        update_segment_back(m);
    }
    m.hovered_item.mode = HoverMode::Segments;
}
