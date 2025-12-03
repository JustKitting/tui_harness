use criterion::{Criterion, black_box, criterion_group, criterion_main};
use screenshot_tool::{
    snapshot::capture::capture_display_screenshot, snapshot::types::SnapshotConfig,
};

fn benchmark_screenshot(c: &mut Criterion) {
    let config = SnapshotConfig::default();

    c.bench_function("screenshot_capture", |b| {
        b.iter(|| {
            let result = unsafe { capture_display_screenshot(black_box(&config)) };
            assert!(result.is_ok());
        })
    });
}

criterion_group!(benches, benchmark_screenshot);
criterion_main!(benches);
