use crate::model::{Model, VideoMetadata};
use std::error::Error;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

const NUM_COLOR_CHANNELS: i32 = 3;

pub struct FrameIterator {
    pub video_path: String,
    render_width: i32,
    render_height: i32,
    stdout: BufReader<std::process::ChildStdout>,
    pixel_buffer: Vec<u8>,
    pub num_frames_rendered: u32,
    pub last_timestamp_secs: f64,
}

impl FrameIterator {
    pub fn create_process(
        path: &str,
        start: f64,
        width: i32,
        height: i32,
    ) -> Result<std::process::ChildStdout, Box<dyn Error>> {
        let mut proc = std::process::Command::new("ffmpeg")
            .args(["-ss", &format!("{:.3}", start)])
            .args(["-i", path])
            .args(["-pix_fmt", "rgb24"])
            .args(["-f", "rawvideo"])
            .args(["-vf", &format!("scale={}:{}", width, height)]) // scale to required size
            .args(["pipe:"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()?;

        proc.stdout.take().ok_or("failed to get stdout".into())
    }

    pub fn new(path: String, w: i32, h: i32) -> Result<Self, Box<dyn Error>> {
        let raw_stdout = Self::create_process(&path, 0.0, w, h)?;
        let stdout =
            BufReader::with_capacity((w * h * NUM_COLOR_CHANNELS) as usize * 2, raw_stdout);
        Ok(Self {
            video_path: path,
            render_width: w,
            render_height: h,
            stdout,
            pixel_buffer: vec![0u8; (w * h * NUM_COLOR_CHANNELS) as usize],
            num_frames_rendered: 0,
            last_timestamp_secs: 0.0,
        })
    }

    pub fn resize(&mut self, w: i32, h: i32) {
        self.render_width = w;
        self.render_height = h;
        self.pixel_buffer = vec![0u8; (w * h * NUM_COLOR_CHANNELS) as usize];
        let ts = self.last_timestamp_secs;
        let _ = self.goto(ts);
    }

    pub fn take_frame(&mut self) -> Result<image::RgbImage, Box<dyn Error>> {
        self.stdout.read_exact(&mut self.pixel_buffer)?;
        self.num_frames_rendered += 1;

        let data = std::mem::replace(
            &mut self.pixel_buffer,
            vec![0u8; (self.render_width * self.render_height * NUM_COLOR_CHANNELS) as usize],
        );
        image::RgbImage::from_raw(self.render_width as u32, self.render_height as u32, data)
            .ok_or("failed to create image".into())
    }

    pub fn skip_frames(&mut self, n: u32) -> Result<image::RgbImage, Box<dyn Error>> {
        for _ in 0..n.saturating_sub(1) {
            let result = self.stdout.read_exact(&mut self.pixel_buffer);
            if result.is_err() {
                return Err("End of stream".into());
            }
        }
        self.take_frame()
    }

    pub fn goto(&mut self, ts: f64) -> Result<image::RgbImage, Box<dyn Error>> {
        let raw_stdout =
            Self::create_process(&self.video_path, ts, self.render_width, self.render_height)?;
        self.stdout = BufReader::with_capacity(
            (self.render_width * self.render_height * NUM_COLOR_CHANNELS) as usize * 2,
            raw_stdout,
        );
        self.num_frames_rendered = 0; // reset frame count for FPS after seek
        self.last_timestamp_secs = ts;
        self.take_frame()
    }
}

pub fn get_ffprobe_video_metadata(video_filepath: &str) -> Result<VideoMetadata, Box<dyn Error>> {
    let probe_process = std::process::Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0"])
        .args([
            "-show_entries",
            "stream=width,height,r_frame_rate:format=duration",
        ])
        .args(["-print_format", "compact=print_section=0:item_sep=,"])
        .arg(video_filepath)
        .output()
        .map_err(|e| format!("ffprobe failed: {}", e))?;

    let plain_output = String::from_utf8(probe_process.stdout)?;
    let single_line = plain_output.trim().replace('\n', ",");

    let mut props = std::collections::HashMap::new();
    for part in single_line.split(',') {
        if let Some((k, v)) = part.split_once('=') {
            props.insert(k, v);
        }
    }

    let width = props.get("width").ok_or("missing width")?.parse()?;
    let height = props.get("height").ok_or("missing height")?.parse()?;
    let (num, den) = props
        .get("r_frame_rate")
        .ok_or("missing fps")?
        .split_once('/')
        .ok_or("bad fps")?;
    let fps = num.parse::<f64>()? / den.parse::<f64>()?;
    let duration = props.get("duration").ok_or("missing duration")?.parse()?;

    Ok(VideoMetadata {
        width,
        height,
        fps,
        seconds_per_frame: 1.0 / fps,
        duration_secs: duration,
    })
}

pub fn process_final_output(model: &Model) -> Result<(), Box<dyn Error>> {
    // no output if no markers AND the first segment is NOT included.
    if model.markers.is_empty() && model.segments_included.get(0).copied().unwrap_or(true) == false
    {
        println!("No segments were marked for inclusion. Exiting without output.");
        return Ok(());
    }

    println!("Processing segments...");

    let mut marks = model.markers.clone();
    // add start and end of the video as bounds.
    marks.insert(0, 0.0);
    marks.push(model.video_metadata.duration_secs);

    let path = PathBuf::from(&model.frame_iterator.video_path);
    let dir = path.parent().unwrap_or(Path::new("."));
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("mp4");
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");

    let included_segments: Vec<(f64, f64, usize)> = marks
        .windows(2)
        .enumerate()
        .filter_map(|(i, w)| {
            if model.segments_included.get(i).copied().unwrap_or(false) {
                Some((w[0], w[1], i))
            } else {
                println!("  -> [Excluded] Segment {}: {}s to {}s", i, w[0], w[1]);
                None
            }
        })
        .collect();

    if included_segments.is_empty() {
        println!("No segments were marked for inclusion. Exiting without output.");
        return Ok(());
    }

    // if all segments are included, we can skip ffmpeg processing and just copy the file (or do nothing if single_output is true).
    let total_included_duration: f64 = included_segments.iter().map(|(s, e, _)| e - s).sum();
    let video_duration = model.video_metadata.duration_secs;

    // small error for floating point comparison.
    const DURATION_EPSILON: f64 = 0.001;

    if included_segments.len() == 1
        && included_segments[0].0 < DURATION_EPSILON
        && (included_segments[0].1 - video_duration).abs() < DURATION_EPSILON
    {
        println!("\nAll segments are included. Output would be identical to input.");
        println!("Exiting without performing unnecessary export.");
        return Ok(());
    } else if (total_included_duration - video_duration).abs() < DURATION_EPSILON
        && model.single_output
    {
        // same case, but there are markers
        println!(
            "\nAll segments are included, and single output is selected. Output would be identical to input."
        );
        println!("Exiting without performing unnecessary export.");
        return Ok(());
    }

    // export logic.
    if model.single_output {
        // create temp file list for ffmpeg's "concat"
        let temp_list_path = dir.join(format!("{}_concat_list.txt", name));
        let mut file_list = String::new();
        let mut output_count = 0;

        let mut handles = Vec::new();
        for (start, end, i) in &included_segments {
            // each segment gets a temp file
            let temp_segment_path = dir.join(format!("{}_temp_seg_{}.{}", name, output_count, ext));
            println!(
                " -> [Temp Output {}] Segment {}: {}s to {}s",
                output_count, i, start, end
            );

            file_list.push_str(&format!("file '{}'\n", temp_segment_path.to_string_lossy()));

            let path_clone = model.frame_iterator.video_path.clone();
            let start_clone = *start;
            let end_clone = *end;
            handles.push(std::thread::spawn(move || {
                let mut cmd = std::process::Command::new("ffmpeg");
                cmd.args(["-ss", &format!("{}", start_clone)])
                    .args(["-i", &path_clone])
                    .args(["-to", &format!("{}", end_clone)])
                    .args(["-c", "copy"]) // copy for speed and quality
                    .args(["-y"]) // overwrite temp files
                    .arg(&temp_segment_path);

                println!("Running command: {:?}", cmd);
                cmd.output()
            }));

            output_count += 1;
        }

        let mut success = true;
        for handle in handles {
            match handle.join() {
                Ok(Ok(result)) if result.status.success() => {}
                _ => success = false,
            }
        }

        if !success {
            eprintln!("Error creating temp segments.");
            // clean up temp files
            for j in 0..output_count {
                let _ = std::fs::remove_file(dir.join(format!("{}_temp_seg_{}.{}", name, j, ext)));
            }
            let _ = std::fs::remove_file(&temp_list_path);
            return Err(Box::from("Failed to create temporary segment files."));
        }

        // write the file list
        std::fs::write(&temp_list_path, file_list).expect("Unable to write file list");

        // do ffmpeg concat.
        let final_output_filename = dir.join(format!("{}_rve_0.{}", name, ext));
        println!(
            "\nConcatenating {} segments into a single file: {}",
            output_count,
            final_output_filename.to_string_lossy()
        );

        let mut cmd = std::process::Command::new("ffmpeg");
        cmd.args(["-f", "concat"])
            .args(["-safe", "0"])
            .args(["-i", &temp_list_path.to_string_lossy().to_string()])
            .args(["-c", "copy"])
            .args(["-y"])
            .arg(&final_output_filename);

        println!("Running command: {:?}", cmd);
        let result = cmd.output();

        if result.is_err() || !result.unwrap().status.success() {
            eprintln!("Error during final concatenation.");
        }

        // clean up temporary files and list
        println!("Cleaning up temporary files...");
        for i in 0..output_count {
            let temp_segment_path = dir.join(format!("{}_temp_seg_{}.{}", name, i, ext));
            let _ = std::fs::remove_file(temp_segment_path);
        }
        let _ = std::fs::remove_file(temp_list_path);

        println!(
            "Done. Single file outputted: {}",
            final_output_filename.to_string_lossy()
        );
        Ok(())
    } else {
        // multi file
        let mut output_count = 0;
        let mut handles = Vec::new();
        for (start, end, i) in included_segments {
            let output_filename = dir.join(format!("{}_rve_{}.{}", name, output_count, ext));
            println!(
                " -> [Output {}] Segment {}: {}s to {}s",
                output_count, i, start, end
            );

            let path_clone = model.frame_iterator.video_path.clone();
            handles.push(std::thread::spawn(move || {
                let mut cmd = std::process::Command::new("ffmpeg");
                cmd.args(["-ss", &format!("{}", start)])
                    .args(["-i", &path_clone])
                    .args(["-to", &format!("{}", end)])
                    .args(["-c", "copy"])
                    .arg(&output_filename);

                println!("Running command: {:?}", cmd);
                cmd.output()
            }));

            output_count += 1;
        }

        let mut success = true;
        for handle in handles {
            match handle.join() {
                Ok(Ok(result)) if result.status.success() => {}
                _ => success = false,
            }
        }

        if !success {
            eprintln!("Error during multi-file segment export for some segment.");
        }
        println!(
            "Done. {} segments outputted (multiple files).",
            output_count
        );
        Ok(())
    }
}
