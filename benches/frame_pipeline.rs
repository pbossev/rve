use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use rve::ffmpeg::FrameIterator;

const TEST_VIDEO: &str = "test_video.mp4";
const RENDER_W: i32 = 160;
const RENDER_H: i32 = 90;

fn bench_frame_decode(c: &mut Criterion) {
    let mut group = c.benchmark_group("frame_decode");
    group.throughput(Throughput::Elements(1)); // 1 frame

    group.bench_function("take_frame_lowres", |b| {
        let mut iter = FrameIterator::new(TEST_VIDEO.into(), RENDER_W, RENDER_H)
            .expect("Failed to create FrameIterator");

        b.iter(|| iter.take_frame().expect("Failed to take frame"));
    });

    group.finish();
}

criterion_group!(benches, bench_frame_decode);
criterion_main!(benches);
