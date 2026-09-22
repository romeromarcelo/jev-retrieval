//! Windowing invariant tests: 1-based overlapping ranges, and UTF-8-safe
//! splitting of pathological minified files into bounded windows and request
//! batches.

use jevr::windows::{
    window_batches, windows, MAX_REQUEST_SOURCE_BYTES, MAX_WINDOWS_PER_REQUEST, MAX_WINDOW_BYTES,
};

#[test]
fn windows_have_one_based_ranges_and_overlap() {
    let text = (1..=180)
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    let result = windows(&text);
    assert_eq!((result[0].start, result[0].end), (1, 100));
    assert_eq!((result[1].start, result[1].end), (81, 180));
}

#[test]
fn large_minified_files_are_split_into_bounded_requests() {
    let text = "é".repeat(MAX_WINDOW_BYTES * 8);
    let windows = windows(&text);
    assert!(windows.len() > 1);
    assert_eq!(
        windows
            .iter()
            .map(|window| window.snippet.as_str())
            .collect::<String>(),
        text
    );
    assert!(windows.iter().all(|window| {
        window.start == 1
            && window.end == 1
            && window.snippet.len() <= MAX_WINDOW_BYTES
            && window.snippet.is_char_boundary(window.snippet.len())
    }));
    assert!(window_batches(&windows).iter().all(|batch| {
        batch.len() <= MAX_WINDOWS_PER_REQUEST
            && batch
                .iter()
                .map(|window| window.snippet.len())
                .sum::<usize>()
                <= MAX_REQUEST_SOURCE_BYTES
    }));
}
