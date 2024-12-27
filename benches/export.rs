use criterion::{Criterion, criterion_group, criterion_main};
use std::process::Command;
use std::time::Instant;

const INPUT: &str = "test_video.mp4";

fn run_sequential_export(segments: &[(f64, f64)]) -> std::time::Duration {
    let start = Instant::now();
    for (i, (ss, to)) in segments.iter().enumerate() {
        Command::new("ffmpeg")
            .args([
                "-y",
                "-ss",
                &ss.to_string(),
                "-i",
                INPUT,
                "-to",
                &to.to_string(),
                "-c",
                "copy",
                &format!("/tmp/rve_bench_seg_{i}.mp4"),
            ])
            .output()
            .unwrap();
    }
    start.elapsed()
}

fn run_parallel_export(segments: &[(f64, f64)]) -> std::time::Duration {
    use std::thread;
    let start = Instant::now();
    let handles: Vec<_> = segments
        .iter()
        .enumerate()
        .map(|(i, (ss, to))| {
            let (ss, to) = (*ss, *to);
            thread::spawn(move || {
                Command::new("ffmpeg")
                    .args([
                        "-y",
                        "-ss",
                        &ss.to_string(),
                        "-i",
                        INPUT,
                        "-to",
                        &to.to_string(),
                        "-c",
                        "copy",
                        &format!("/tmp/rve_bench_par_{i}.mp4"),
                    ])
                    .output()
                    .unwrap();
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    start.elapsed()
}

fn bench_export(c: &mut Criterion) {
    // 5-second segments
    let segments = vec![(0.0, 5.0), (5.0, 10.0), (10.0, 15.0), (15.0, 20.0)];

    let mut group = c.benchmark_group("segment_export");
    group.sample_size(10); // export is slow; limit samples

    group.bench_function("sequential", |b| {
        b.iter(|| run_sequential_export(&segments))
    });
    group.bench_function("parallel", |b| b.iter(|| run_parallel_export(&segments)));

    group.finish();
}

criterion_group!(benches, bench_export);
criterion_main!(benches);
