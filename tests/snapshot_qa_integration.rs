//! Integration tests for the snapshot capture process

use std::fs;
use std::path::PathBuf;

use cli_vision::snapshot::{MockFramebuffer, CaptureBackend, SnapshotConfig, capture_with_backend};

#[test]
fn test_mock_capture_process() {
    let screenshots_dir = PathBuf::from("./test_screenshots");
    fs::create_dir_all(&screenshots_dir).expect("Failed to create screenshots dir");

    let mut snapshot_config = SnapshotConfig::default();
    snapshot_config.output_dir = screenshots_dir.clone();
    snapshot_config.include_metadata = true;

    let mut backend = MockFramebuffer::with_color(800, 600, [50, 50, 50]);
    backend.draw_rect(10, 10, 100, 30, [255, 255, 255]);
    backend.draw_text(15, 15, "Test Header", [0, 0, 0], [255, 255, 255]);

    let result = capture_with_backend(&mut backend, &snapshot_config);
    assert!(result.is_ok(), "Capture failed: {:?}", result.err());

    let snapshot = result.unwrap();
    assert!(snapshot.image_path.exists(), "Screenshot file not created");

    // Cleanup
    let _ = fs::remove_file(&snapshot.image_path);
    let _ = fs::remove_dir_all(&screenshots_dir);
}

#[test]
fn test_mock_framebuffer_operations() {
    let mut fb = MockFramebuffer::new(100, 100);

    // Test fill
    fb.fill([128, 128, 128]);
    assert_eq!(fb.get_pixel(50, 50), [128, 128, 128]);

    // Test draw_rect
    fb.draw_rect(10, 10, 20, 20, [255, 0, 0]);
    assert_eq!(fb.get_pixel(15, 15), [255, 0, 0]);

    // Test to_png roundtrip
    let png_data = fb.to_png().expect("Failed to create PNG");
    let fb2 = MockFramebuffer::from_png_bytes(&png_data).expect("Failed to load PNG");
    assert_eq!(fb2.width(), fb.width());
    assert_eq!(fb2.height(), fb.height());
}

#[test]
fn test_capture_creates_metadata() {
    let screenshots_dir = PathBuf::from("./test_screenshots_meta");
    fs::create_dir_all(&screenshots_dir).expect("Failed to create screenshots dir");

    let mut snapshot_config = SnapshotConfig::default();
    snapshot_config.output_dir = screenshots_dir.clone();
    snapshot_config.include_metadata = true;
    snapshot_config.include_manifest = true;

    let mut backend = MockFramebuffer::new(640, 480);
    let result = capture_with_backend(&mut backend, &snapshot_config);

    assert!(result.is_ok());
    let snapshot = result.unwrap();

    // Check metadata exists
    assert!(snapshot.metadata.is_some());
    let meta = snapshot.metadata.as_ref().unwrap();
    assert!(meta.get("width").is_some());
    assert!(meta.get("height").is_some());
    assert!(meta.get("source").is_some());

    // Cleanup
    let _ = fs::remove_file(&snapshot.image_path);
    let manifest_path = snapshot.image_path.with_extension("json");
    let _ = fs::remove_file(&manifest_path);
    let _ = fs::remove_dir_all(&screenshots_dir);
}
