use criterion::{Criterion, criterion_group, criterion_main};
use rve::model::{DisplayMode, Model};
use rve::view;
use std::io::sink;

const RENDER_W: u16 = 160;
const RENDER_H: u16 = 45;

fn bench_renderers(c: &mut Criterion) {
    let mut group = c.benchmark_group("renderers");
    group.sample_size(10);

    // create a dummy image
    let mut img = image::RgbImage::new(RENDER_W as u32, (RENDER_H * 2) as u32);
    for (x, y, pixel) in img.enumerate_pixels_mut() {
        *pixel = image::Rgb([(x % 255) as u8, (y % 255) as u8, ((x + y) % 255) as u8]);
    }

    let mut model = Model::new(
        "test_video.mp4".into(),
        RENDER_W,
        RENDER_H,
        DisplayMode::LowResBlock,
        false,
        false,
    )
    .unwrap();
    model.current_frame = Some(img.clone());

    // NOTE: viuer renderer benchmarks are artificially fast because we use `gag` to intercept stdout.
    // In the real world, viuer writes ~140KB of ANSI strings to the terminal every frame.
    // The terminal emulator parsing and drawing that massive string is the *true* bottleneck.
    // Since gag redirects stdout to a pipe, it skips the terminal emulator entirely.
    // The differential renderer's massive real-world performance gain comes from reducing
    // that terminal I/O from 140KB to ~1KB per frame (when few pixels change).

    group.bench_function("differential_0_percent", |b| {
        b.iter(|| {
            let mut out = sink();
            view::render_differential(
                &mut model.terminal_state,
                false,
                &mut out,
                &img,
                0,
                RENDER_W,
                RENDER_H,
            )
            .unwrap();
        });
    });

    group.bench_function("differential_10_percent", |b| {
        b.iter_batched(
            || {
                // clone the model state so we own it for the iteration
                let mut state = model.terminal_state.clone();
                // reset state to match image perfectly
                view::render_differential(
                    &mut state,
                    true,
                    &mut sink(),
                    &img,
                    0,
                    RENDER_W,
                    RENDER_H,
                )
                .unwrap();
                // dirty 10% of the blocks
                let dirty_count = state.blocks.len() / 10;
                for i in 0..dirty_count {
                    state.blocks[i] = (image::Rgb([0, 0, 0]), image::Rgb([0, 0, 0]));
                }
                state
            },
            |mut state| {
                let mut out = sink();
                view::render_differential(&mut state, false, &mut out, &img, 0, RENDER_W, RENDER_H)
                    .unwrap();
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.bench_function("differential_100_percent", |b| {
        b.iter(|| {
            let mut out = sink();
            view::render_differential(
                &mut model.terminal_state,
                true, // forces 100% redraw
                &mut out,
                &img,
                0,
                RENDER_W,
                RENDER_H,
            )
            .unwrap();
        });
    });

    #[cfg(feature = "viuer")]
    group.bench_function("viuer_renderer", |b| {
        b.iter(|| {
            let _gag = gag::Gag::stdout().unwrap();
            let cfg = viuer::Config {
                x: 0,
                y: 0,
                width: Some(RENDER_W as u32),
                height: Some(RENDER_H as u32),
                absolute_offset: true,
                use_kitty: false,
                use_iterm: false,
                ..Default::default()
            };
            viuer::print(&image::DynamicImage::ImageRgb8(img.clone()), &cfg).unwrap();
        });
    });

    group.finish();
}

criterion_group!(benches, bench_renderers);
criterion_main!(benches);
