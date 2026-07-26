use crate::model::{
    DisplayMode, HoverMode, MAX_HR_WIDTH, Model, TIMELINE_ROW, UI_HEIGHT, VideoMetadata,
};
use crossterm::{
    cursor::{MoveTo, MoveToColumn, MoveToNextLine},
    queue,
    style::{self, Print, SetAttribute},
    terminal,
};
use std::io::Write;

fn fmt_time(s: f64) -> String {
    format!("{}:{:02}", (s / 60.0) as u32, (s % 60.0) as u32)
}

/// uhhh calculate the render size
pub fn calculate_render_size(
    term_cols: u16,
    term_rows: u16,
    video_aspect: f64,
    meta: &VideoMetadata,
    mode: DisplayMode,
) -> (i32, i32) {
    let term_view_rows = term_rows.saturating_sub(UI_HEIGHT);

    let term_pixel_h = term_view_rows as f64 * 2.0; // half-blocks
    let term_pixel_w = term_cols as f64; // blocks
    let term_aspect = term_pixel_w / term_pixel_h;

    let (w, h) = if term_aspect > video_aspect {
        (term_pixel_h * video_aspect, term_pixel_h)
    } else {
        (term_pixel_w, term_pixel_w / video_aspect)
    };

    if mode == DisplayMode::LowResBlock {
        (w.round() as i32, ((h / 2.0).round() as i32) * 2)
    } else {
        let original_w = meta.width as f64;
        let original_h = meta.height as f64;

        if original_w <= MAX_HR_WIDTH as f64 {
            (original_w as i32, original_h as i32)
        } else {
            let scale = MAX_HR_WIDTH as f64 / original_w;
            let scaled_h = (original_h * scale).round() as i32;
            (MAX_HR_WIDTH as i32, scaled_h)
        }
    }
}

pub fn view(m: &mut Model, out: &mut impl Write) -> std::io::Result<()> {
    if m.needs_to_clear {
        queue!(out, terminal::Clear(terminal::ClearType::All))?;
        let _ = write!(out, "\x1b_Ga=d,d=A\x1b\\");
    }

    let img_width_chars;
    let x_offset;
    let ui_start_row;

    // calculate the character-cell dimensions of the video area.
    let video_aspect = m.video_metadata.width as f64 / m.video_metadata.height as f64;
    let (render_w, render_h) = calculate_render_size(
        m.terminal_cols,
        m.terminal_rows,
        video_aspect,
        &m.video_metadata,
        DisplayMode::LowResBlock, // LowResBlock because character-cell
    );
    let char_w = render_w as u16;
    let char_h = (render_h / 2) as u16;

    // use these dimensions to set the layout for BOTH modes.
    img_width_chars = char_w;
    x_offset = m.terminal_cols.saturating_sub(img_width_chars) / 2;
    ui_start_row = char_h;

    if let Some(img) = &m.current_frame {
        if m.display_mode == DisplayMode::HighResPixel {
            render_kitty(img, x_offset, 0, char_w, char_h, out)?;
        } else {
            render_differential(
                &mut m.terminal_state,
                m.needs_to_clear,
                out,
                img,
                x_offset,
                char_w as u16,
                char_h as u16,
            )?;
        }
    }

    queue!(out, MoveTo(0, ui_start_row))?;

    // time and fps first
    let total_frames = (m.video_metadata.duration_secs * m.video_metadata.fps).round() as u32;
    let time_str = format!(
        " {} / {} [{}/{}] {}",
        fmt_time(m.frame_number as f64 / m.video_metadata.fps),
        fmt_time(m.video_metadata.duration_secs),
        m.frame_number,
        total_frames,
        if m.paused { "||" } else { ">>" }
    );
    let fps_str = format!(
        "fps: {}",
        m.recent_fps.map_or("--".into(), |f| format!("{:<2.0}", f))
    );

    let used = time_str.len() + fps_str.len();
    let padding = (m.terminal_cols as usize).saturating_sub(used);
    queue!(
        out,
        Print(fps_str),
        Print(" ".repeat(padding)),
        Print(time_str),
        MoveToNextLine(1)
    )?;

    // timeline bar
    let bar_len = m.terminal_cols as usize;
    let timeline_ui_row = ui_start_row + TIMELINE_ROW;

    // segment inclusion background if needed
    if m.needs_to_clear {
        // clear area for timeline
        queue!(
            out,
            MoveTo(0, timeline_ui_row.saturating_sub(1)),
            Print(" ".repeat(bar_len)),
        )?;
        queue!(out, MoveTo(0, timeline_ui_row), Print(" ".repeat(bar_len)),)?;
    }

    let included_color = style::Color::Rgb {
        r: 50,
        g: 200,
        b: 50,
    };
    let excluded_color = style::Color::Rgb {
        r: 200,
        g: 50,
        b: 50,
    };
    let default_color = style::Color::Reset;

    let total_secs = m.video_metadata.duration_secs;
    let mut marks = m.markers.clone();
    marks.insert(0, 0.0); // start
    marks.push(m.video_metadata.duration_secs);

    // draw segments with background color
    for (i, w) in marks.windows(2).enumerate() {
        let is_included = m.segments_included.get(i).copied().unwrap_or(true);
        let start_ts = w[0];
        let end_ts = w[1];

        // figure out start and end character position on the timeline bar
        let start_pos = (bar_len as f64 * start_ts / total_secs).round() as u16;
        let end_pos = (bar_len as f64 * end_ts / total_secs).round() as u16;
        let len = end_pos.saturating_sub(start_pos);

        if len > 0 {
            let color = if is_included {
                included_color
            } else {
                excluded_color
            };
            queue!(
                out,
                MoveTo(
                    start_pos.min(m.terminal_cols.saturating_sub(1)),
                    timeline_ui_row
                ),
                style::SetBackgroundColor(color),
                Print(
                    " ".repeat(
                        len.min(
                            (m.terminal_cols.saturating_sub(start_pos) as usize)
                                .try_into()
                                .unwrap()
                        )
                        .into()
                    )
                ),
            )?;
        }
    }
    queue!(out, style::SetBackgroundColor(default_color))?;

    // draw markers
    for ts in &m.markers {
        let pos = (bar_len as f64 * ts / total_secs).round() as u16;
        let clamped_pos = pos.min(m.terminal_cols.saturating_sub(1));

        queue!(
            out,
            MoveTo(clamped_pos, timeline_ui_row),
            style::SetForegroundColor(style::Color::White),
            Print("|"),
            style::SetForegroundColor(style::Color::Reset),
        )?;
    }

    // draw the playhead indicator
    let head_pos =
        (bar_len as f64 * m.frame_number as f64 / m.video_metadata.fps / total_secs).round() as u16;
    let clamped_head_pos = head_pos.min(m.terminal_cols.saturating_sub(1));

    // determine color for the playhead (included/excluded)
    let segment_idx = m.hovered_item.position;
    let is_included = m
        .segments_included
        .get(segment_idx)
        .copied()
        .unwrap_or(true);
    let playhead_color = if is_included {
        style::Color::Rgb {
            r: 50,
            g: 200,
            b: 50,
        }
    } else {
        style::Color::Rgb {
            r: 200,
            g: 50,
            b: 50,
        }
    };

    // clear the old playhead
    let prev_head_pos =
        (bar_len as f64 * m.prev_frame_number as f64 / m.video_metadata.fps / total_secs).round()
            as u16;
    let clamped_prev_pos = prev_head_pos.min(m.terminal_cols.saturating_sub(1));
    if m.frame_number != m.prev_frame_number || m.needs_to_clear {
        queue!(
            out,
            MoveTo(clamped_prev_pos, timeline_ui_row.saturating_sub(1)),
            Print(" "),
        )?;
    }

    // print new playhead
    queue!(
        out,
        MoveTo(clamped_head_pos, timeline_ui_row.saturating_sub(1)),
        style::SetForegroundColor(playhead_color),
        Print("v"),
        style::SetForegroundColor(default_color),
    )?;

    // move cursor down for the rest of the UI (4 rows below the video render area)
    queue!(out, MoveTo(0, ui_start_row + 4))?;

    // segment/marker info and help toggle
    let (nm, ns) = (m.markers.len(), m.markers.len() + 1);
    let status_str = if is_included { "INCLUDED" } else { "EXCLUDED" };
    let output_mode = if m.single_output {
        "Single-File"
    } else {
        "Multi-File"
    };

    let res_mode = if m.display_mode == DisplayMode::LowResBlock {
        "Low-Res (Block)"
    } else if m.high_res_available && m.display_mode == DisplayMode::HighResPixel {
        // only if available
        "High-Res (Pixel)"
    } else {
        "Low-Res (Block)"
    };

    let vol_str = format!("{:.0}%", m.audio_player.volume * 100.0);
    let exp_vol_str = format!("{:.0}%", m.audio_player.export_volume * 100.0);

    let segment_info = match m.hovered_item.mode {
        HoverMode::Segments => format!(
            "Segment {} of {} [{}] | Output: {} | Display: {} | Vol: {} | Export Vol: {}",
            m.hovered_item.position + 1,
            ns,
            status_str,
            output_mode,
            res_mode,
            vol_str,
            exp_vol_str,
        ),
        HoverMode::Markers => format!(
            "Marker {} of {} | Output: {} | Display: {} | Vol: {} | Export Vol: {}",
            m.hovered_item.position + 1,
            nm,
            output_mode,
            res_mode,
            vol_str,
            exp_vol_str,
        ),
    };

    let segment_str = format!(
        "{:width$}",
        segment_info,
        width = m.terminal_cols.saturating_sub(8) as usize
    );
    let help_str = "help ?";
    let used = segment_str.len() + help_str.len();
    let padding = (m.terminal_cols as usize).saturating_sub(used);
    queue!(
        out,
        Print(segment_str),
        Print(" ".repeat(padding)),
        Print(help_str),
        MoveToNextLine(1)
    )?;

    // controls/helps
    if !m.hide_controls {
        let mut seg_controls = vec![
            "v mark/unmark",
            "t toggle segment",
            "i toggle output",
            "+/- vol",
            "a/A export vol",
            "s save/output",
        ];

        // only if high-res mode available
        if m.high_res_available {
            seg_controls.push("r toggle res");
        }

        let nav_controls = [
            if nm > 0 { "[ ] jump" } else { "" },
            "←/→ 5s",
            "Alt + ←/→ 30s",
            "Ctrl + ←/→ 1m",
            ",/. skip frame",
            "0-9 jump",
            "Esc/q quit",
        ];

        let seg_controls_line = seg_controls.join(" • ");
        let nav_controls_vec: Vec<&str> = nav_controls
            .iter()
            .cloned()
            .filter(|s| !s.is_empty())
            .collect();
        let nav_controls_line = nav_controls_vec.join(" • ");

        // center
        let total_cols = m.terminal_cols as usize;
        let pad_seg = if total_cols > seg_controls_line.len() {
            (total_cols - seg_controls_line.len()) / 2
        } else {
            0
        };
        let pad_nav = if total_cols > nav_controls_line.len() {
            (total_cols - nav_controls_line.len()) / 2
        } else {
            0
        };

        queue!(
            out,
            MoveToColumn(0),
            Print(" ".repeat(pad_seg)),
            SetAttribute(style::Attribute::Dim),
            Print(seg_controls_line.clone()),
            SetAttribute(style::Attribute::Reset),
            Print(" ".repeat(total_cols.saturating_sub(pad_seg + seg_controls_line.len()))),
            MoveToNextLine(1),
            MoveToColumn(0),
            Print(" ".repeat(pad_nav)),
            SetAttribute(style::Attribute::Dim),
            Print(nav_controls_line.clone()),
            SetAttribute(style::Attribute::Reset),
            Print(" ".repeat(total_cols.saturating_sub(pad_nav + nav_controls_line.len()))),
            MoveToNextLine(1)
        )?;
    } else {
        // clear if controls are hidden
        queue!(
            out,
            MoveToColumn(0),
            Print(" ".repeat(m.terminal_cols as usize)),
            MoveToNextLine(1),
            MoveToColumn(0),
            Print(" ".repeat(m.terminal_cols as usize)),
            MoveToNextLine(1)
        )?;
    }

    if m.exit_prompt {
        if m.display_mode == DisplayMode::HighResPixel {
            let _ = write!(out, "\x1b_Ga=d,d=A\x1b\\");
        }
        let line1 = " Confirm exit? (make sure you save) ";
        let line2 = " [Y]es/[N]o ";

        let center_row = ui_start_row / 2;
        let pad1 = m.terminal_cols.saturating_sub(line1.len() as u16) / 2;
        let pad2 = m.terminal_cols.saturating_sub(line2.len() as u16) / 2;

        queue!(
            out,
            MoveTo(pad1, center_row.saturating_sub(1)),
            style::SetBackgroundColor(style::Color::Red),
            style::SetForegroundColor(style::Color::White),
            SetAttribute(style::Attribute::Bold),
            Print(line1),
            MoveTo(pad2, center_row),
            style::SetBackgroundColor(style::Color::Red),
            style::SetForegroundColor(style::Color::White),
            SetAttribute(style::Attribute::Bold),
            Print(line2),
            SetAttribute(style::Attribute::Reset),
            style::SetBackgroundColor(style::Color::Reset),
            style::SetForegroundColor(style::Color::Reset),
        )?;
    }

    Ok(())
}

pub fn render_differential(
    state: &mut crate::model::TerminalState,
    needs_to_clear: bool,
    out: &mut impl Write,
    img: &image::RgbImage,
    x_offset: u16,
    width: u16,
    height: u16,
) -> std::io::Result<()> {
    let expected_len = (width as usize) * (height as usize);
    if state.blocks.len() != expected_len || state.width != width || needs_to_clear {
        state.width = width;
        state.height = height;
        // reset with black to force redraw on first frame
        state.blocks = vec![(image::Rgb([0, 0, 0]), image::Rgb([0, 0, 0])); expected_len];
        // don't set needs_to_clear here, as it might cause infinite loop of clearing
        // but we must treat all blocks as dirty if we just recreated them
    }

    let mut current_fg: Option<image::Rgb<u8>> = None;
    let mut current_bg: Option<image::Rgb<u8>> = None;

    for row in 0..height {
        let mut cursor_moved = false;

        for col in 0..width {
            let img_x = col as u32;
            let img_y_top = (row as u32) * 2;
            let img_y_bot = img_y_top + 1;

            let top_color = if img_x < img.width() && img_y_top < img.height() {
                *img.get_pixel(img_x, img_y_top)
            } else {
                image::Rgb([0, 0, 0])
            };

            let bot_color = if img_x < img.width() && img_y_bot < img.height() {
                *img.get_pixel(img_x, img_y_bot)
            } else {
                image::Rgb([0, 0, 0])
            };

            let idx = (row as usize) * (width as usize) + (col as usize);
            let prev_colors = state.blocks[idx];

            if needs_to_clear || prev_colors != (top_color, bot_color) {
                if !cursor_moved {
                    queue!(out, MoveTo(x_offset + col, row))?;
                    cursor_moved = true;
                }

                if current_fg != Some(top_color) {
                    queue!(
                        out,
                        style::SetForegroundColor(style::Color::Rgb {
                            r: top_color[0],
                            g: top_color[1],
                            b: top_color[2]
                        })
                    )?;
                    current_fg = Some(top_color);
                }

                if current_bg != Some(bot_color) {
                    queue!(
                        out,
                        style::SetBackgroundColor(style::Color::Rgb {
                            r: bot_color[0],
                            g: bot_color[1],
                            b: bot_color[2]
                        })
                    )?;
                    current_bg = Some(bot_color);
                }

                queue!(out, Print("▀"))?;
                state.blocks[idx] = (top_color, bot_color);
            } else {
                cursor_moved = false;
            }
        }
    }

    queue!(
        out,
        style::SetForegroundColor(style::Color::Reset),
        style::SetBackgroundColor(style::Color::Reset)
    )?;

    Ok(())
}

fn base64_encode(data: &[u8]) -> String {
    const B64_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(B64_CHARS[((triple >> 18) & 63) as usize] as char);
        out.push(B64_CHARS[((triple >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(B64_CHARS[((triple >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(B64_CHARS[(triple & 63) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

pub fn render_kitty(
    img: &image::RgbImage,
    x_offset: u16,
    y_offset: u16,
    char_w: u16,
    char_h: u16,
    out: &mut impl Write,
) -> std::io::Result<()> {
    let width = img.width();
    let height = img.height();
    let raw_bytes = img.as_raw();
    let b64 = base64_encode(raw_bytes);

    queue!(out, MoveTo(x_offset, y_offset))?;

    const CHUNK_SIZE: usize = 4096;
    let bytes = b64.as_bytes();
    let total_len = bytes.len();
    let mut i = 0;
    let mut is_first = true;

    while i < total_len {
        let end = (i + CHUNK_SIZE).min(total_len);
        let chunk_str = std::str::from_utf8(&bytes[i..end]).unwrap();
        let m_val = if end == total_len { 0 } else { 1 };

        if is_first {
            write!(
                out,
                "\x1b_Ga=T,f=24,s={},v={},c={},r={},m={},q=2;{}\x1b\\",
                width, height, char_w, char_h, m_val, chunk_str
            )?;
            is_first = false;
        } else {
            write!(out, "\x1b_Gm={};{}\x1b\\", m_val, chunk_str)?;
        }
        i = end;
    }
    out.flush()?;
    Ok(())
}
