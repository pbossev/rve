use rodio::{buffer::SamplesBuffer, OutputStream, Sink};
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub struct AudioPlayer {
    _stream: Option<OutputStream>,
    sink: Option<Arc<Sink>>,
    epoch: Arc<AtomicU64>,
    pub volume: f32,
    pub export_volume: f32,
}

impl AudioPlayer {
    pub fn new() -> Self {
        // try initializing audio output device
        let (stream, sink) = match OutputStream::try_default() {
            Ok((s, handle)) => match Sink::try_new(&handle) {
                Ok(sink) => (Some(s), Some(Arc::new(sink))),
                Err(_) => (None, None),
            },
            Err(_) => (None, None),
        };

        Self {
            _stream: stream,
            sink,
            epoch: Arc::new(AtomicU64::new(0)),
            volume: 1.0,
            export_volume: 1.0,
        }
    }

    pub fn set_volume(&mut self, vol: f32) {
        self.volume = vol.clamp(0.0, 2.0);
        if let Some(sink) = &self.sink {
            sink.set_volume(self.volume);
        }
    }

    pub fn set_export_volume(&mut self, vol: f32) {
        self.export_volume = vol.clamp(0.0, 2.0);
    }

    pub fn pause(&self) {
        if let Some(sink) = &self.sink {
            sink.pause();
        }
    }

    pub fn play(&self) {
        if let Some(sink) = &self.sink {
            sink.play();
        }
    }

    pub fn seek(&mut self, video_path: &str, start_ts: f64, paused: bool) {
        let sink = match &self.sink {
            Some(s) => s,
            None => return,
        };

        sink.clear();

        // bump epoch to cancel older playback threads
        let current_epoch = self.epoch.fetch_add(1, Ordering::SeqCst) + 1;
        let epoch_clone = self.epoch.clone();
        let path = video_path.to_string();

        if paused {
            sink.pause();
        } else {
            sink.play();
        }

        let sink_handle = Arc::clone(sink);

        // spawn background thread to decode raw pcm audio via ffmpeg
        std::thread::spawn(move || {
            let mut child = match Command::new("ffmpeg")
                .args(["-ss", &format!("{:.3}", start_ts)])
                .args(["-i", &path])
                .args(["-vn"])
                .args(["-f", "s16le"])
                .args(["-ac", "2"])
                .args(["-ar", "44100"])
                .args(["pipe:1"])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(c) => c,
                Err(_) => return,
            };

            let mut stdout = match child.stdout.take() {
                Some(s) => s,
                None => return,
            };

            let mut raw_buf = [0u8; 8192];
            while epoch_clone.load(Ordering::Relaxed) == current_epoch {
                let n = match stdout.read(&mut raw_buf) {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(_) => break,
                };

                // convert raw bytes to 16-bit pcm samples
                let pcm_samples: Vec<i16> = raw_buf[..n]
                    .chunks_exact(2)
                    .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
                    .collect();

                if pcm_samples.is_empty() {
                    continue;
                }

                let source = SamplesBuffer::new(2, 44100, pcm_samples);
                sink_handle.append(source);

                // throttle feeding samples if audio sink buffer gets full
                while sink_handle.len() > 20 && epoch_clone.load(Ordering::Relaxed) == current_epoch {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }

            let _ = child.kill();
            let _ = child.wait();
        });
    }
}
