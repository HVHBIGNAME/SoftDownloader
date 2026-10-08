#[cfg(windows)]
#[test]
fn native_decoder_preserves_orientation_and_can_restart_at_end_of_file() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/video-colors.mp4");
    let mut decoder = softdownloader::video::Decoder::open(&path).unwrap();
    let first = decoder.next_frame().unwrap().unwrap();
    assert_eq!(first.size, [64, 48]);
    let top = 8 * (first.size[0] * 4) + 8 * 4;
    let bottom = 40 * (first.size[0] * 4) + 8 * 4;
    assert!(
        first.rgba[top] > 220 && first.rgba[top + 2] < 30,
        "top should be red"
    );
    assert!(
        first.rgba[bottom] < 30 && first.rgba[bottom + 2] > 220,
        "bottom should be blue"
    );
    let mut count = 1;
    while decoder.next_frame().unwrap().is_some() {
        count += 1;
        assert!(count <= 8, "test clip must reach end of stream");
    }
    assert!(count >= 4);
    decoder.rewind().unwrap();
    let restarted = decoder.next_frame().unwrap().unwrap();
    assert_eq!(restarted.rgba, first.rgba);
    assert_eq!(restarted.timestamp, first.timestamp);
    let folder = tempfile::tempdir().unwrap();
    let unicode = folder.path().join("видео с пробелами.mp4");
    std::fs::copy(&path, &unicode).unwrap();
    let mut copied =
        softdownloader::video::Decoder::open(&unicode.canonicalize().unwrap()).unwrap();
    assert_eq!(copied.next_frame().unwrap().unwrap().rgba, first.rgba);
}
