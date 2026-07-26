//! Application entry point and main event loop runner for `rve`.

use crossterm::{
    event::{Event, KeyCode, KeyEvent, KeyModifiers, poll, read},
    execute, terminal,
};
use rve::ffmpeg;
use rve::model::{DisplayMode, Model};
use rve::update;
use rve::view;
use std::io::stdout;
use std::time::Duration;
use std::{error::Error, io::Write};

/// Detects terminal support for high-resolution graphics protocols (e.g., Kitty, WezTerm, Ghostty).
fn detect_high_res_support() -> bool {
    let env_contains = |var: &str, terms: &[&str]| {
        std::env::var(var).map_or(false, |val| {
            let lower = val.to_lowercase();
            terms.iter().any(|t| lower.contains(t))
        })
    };

    env_contains("TERM", &["kitty", "wezterm", "ghostty", "konsole"])
        || env_contains("TERM_PROGRAM", &["kitty", "wezterm", "ghostty", "iterm", "konsole"])
        || env_contains("LC_TERMINAL", &["kitty", "iterm"])
        || std::env::var_os("KITTY_WINDOW_ID").is_some()
        || std::env::var_os("WEZTERM_PANE").is_some()
        || std::env::var_os("GHOSTTY_RESOURCES_DIR").is_some()
}

/// Parses command-line flags and initializes the `Model`.
fn init() -> Result<Model, String> {
    let (cols, rows) = terminal::size().map_err(|e| format!("terminal size: {}", e))?;
    let mut high_res_available = detect_high_res_support();

    let args: Vec<_> = std::env::args().collect();
    let mut single_output = false;
    let mut video_path = None;
    let mut initial_mode = DisplayMode::LowResBlock;

    for arg in &args[1..] {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("USAGE: rve <filepath> [--single-output | -s] [--high-res | -r]");
                std::process::exit(0);
            }
            "--single-output" | "-s" => single_output = true,
            "--high-res" | "-r" => {
                high_res_available = true;
                initial_mode = DisplayMode::HighResPixel;
            }
            _ if !arg.starts_with('-') => video_path = Some(arg.clone()),
            _ => {}
        }
    }

    let video_path = video_path.ok_or_else(|| "USAGE: rve <filepath>".to_string())?;
    Model::new(
        video_path,
        cols,
        rows,
        initial_mode,
        single_output,
        high_res_available,
    )
    .map_err(|e| e.to_string())
}

/// Initializes terminal raw mode and enters the main event render loop.
fn main() -> Result<(), Box<dyn Error>> {
    let mut model = match init() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Error initializing: {}", e);
            std::process::exit(1);
        }
    };

    let mut stdout = stdout();
    terminal::enable_raw_mode()?;
    execute!(
        stdout,
        terminal::EnterAlternateScreen,
        terminal::DisableLineWrap,
        crossterm::cursor::Hide
    )?;

    // Initial draw
    if update::update(&mut model, Event::Key(KeyCode::Null.into()))? {
        view::view(&mut model, &mut stdout)?;
        stdout.flush()?;
        model.needs_to_clear = false;
    }

    loop {
        let timeout = if model.paused {
            Duration::from_millis(50)
        } else {
            let spf = model.video_metadata.seconds_per_frame;
            let elapsed = model.prev_instant.elapsed().as_secs_f64();
            let remaining = spf - elapsed;
            if remaining > 0.001 {
                Duration::from_secs_f64(remaining - 0.001)
            } else {
                Duration::from_millis(0)
            }
        };

        let event = if poll(timeout)? {
            read()?
        } else if !model.paused {
            Event::Key(KeyEvent::new(KeyCode::Null, KeyModifiers::NONE))
        } else {
            continue;
        };

        if update::update(&mut model, event)? {
            view::view(&mut model, &mut stdout)?;
            stdout.flush()?;
            if model.needs_to_clear {
                model.needs_to_clear = false;
            }
        }

        if model.is_saving {
            model.audio_player.pause();
            model.is_saving = false;
            execute!(
                stdout,
                terminal::LeaveAlternateScreen,
                crossterm::cursor::Show
            )?;
            terminal::disable_raw_mode()?;

            if let Err(e) = ffmpeg::process_final_output(&model) {
                eprintln!("Error saving: {}", e);
            }

            println!("\nPress Enter to return to rve...");
            let mut buf = String::new();
            let _ = std::io::stdin().read_line(&mut buf);

            terminal::enable_raw_mode()?;
            execute!(
                stdout,
                terminal::EnterAlternateScreen,
                terminal::DisableLineWrap,
                crossterm::cursor::Hide
            )?;
            model.needs_to_clear = true;
            model.prev_instant = std::time::Instant::now();
            view::view(&mut model, &mut stdout)?;
            stdout.flush()?;
        }

        if model.should_exit {
            model.audio_player.pause();
            break;
        }
    }

    execute!(
        stdout,
        terminal::EnableLineWrap,
        terminal::LeaveAlternateScreen,
        crossterm::cursor::Show
    )?;
    terminal::disable_raw_mode()?;

    println!("Exited rve.");

    Ok(())
}
