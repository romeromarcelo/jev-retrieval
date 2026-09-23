//! 100/20 line windowing + UTF-8-safe byte caps + request batching.
//!
//! Overlapping windows keep recall from being window-limited (a match that
//! straddles one boundary lands whole inside the neighbor), and the UTF-8-safe
//! chunking correctly splits pathological minified/single-line files without
//! breaking char boundaries. The byte caps bound each request under the
//! measured 24-window / 24 KB envelope where server latency stays 0.3–0.8 s
//! regardless of batch size (docs/BENCHMARKS.md), which is what makes full
//! request-level concurrency effective.

/// Lines per verification window.
pub const WINDOW: usize = 100;
/// Lines shared between adjacent windows, so a match straddling one boundary
/// lands whole inside the neighbor.
pub const OVERLAP: usize = 20;
/// Hard byte cap for a single window (UTF-8-safe split of minified files).
pub const MAX_WINDOW_BYTES: usize = 12_000;
/// Measured request envelope: max source bytes per request (docs/BENCHMARKS.md).
pub const MAX_REQUEST_SOURCE_BYTES: usize = 24_000;
/// Measured request envelope: max windows per request (docs/BENCHMARKS.md).
pub const MAX_WINDOWS_PER_REQUEST: usize = 24;

/// One verification window: 1-based line range plus its text.
#[derive(Clone)]
pub struct CodeWindow {
    pub start: usize,
    pub end: usize,
    pub snippet: String,
}

/// Split `text` into overlapping ~100-line windows with 1-based line ranges.
pub fn windows(text: &str) -> Vec<CodeWindow> {
    let lines: Vec<&str> = text.lines().collect();
    let mut result = Vec::new();
    let mut start = 0;
    while start < lines.len() {
        let end = (start + WINDOW).min(lines.len());
        push_bounded_windows(&mut result, &lines[start..end], start + 1);
        if end == lines.len() {
            break;
        }
        start = end.saturating_sub(OVERLAP);
    }
    result
}

/// Append windows for one line-slice, splitting whenever the byte cap would be
/// exceeded so no single window can blow past `MAX_WINDOW_BYTES`.
pub fn push_bounded_windows(result: &mut Vec<CodeWindow>, lines: &[&str], first_line: usize) {
    let mut snippet = String::new();
    let mut start = first_line;
    let mut end = first_line;
    let mut has_lines = false;

    for (offset, line) in lines.iter().enumerate() {
        let line_number = first_line + offset;
        if line.len() > MAX_WINDOW_BYTES {
            if has_lines {
                result.push(CodeWindow {
                    start,
                    end,
                    snippet: std::mem::take(&mut snippet),
                });
                has_lines = false;
            }
            for part in utf8_chunks(line, MAX_WINDOW_BYTES) {
                result.push(CodeWindow {
                    start: line_number,
                    end: line_number,
                    snippet: part.to_owned(),
                });
            }
            continue;
        }

        let added = line.len() + usize::from(has_lines);
        if has_lines && snippet.len() + added > MAX_WINDOW_BYTES {
            result.push(CodeWindow {
                start,
                end,
                snippet: std::mem::take(&mut snippet),
            });
            has_lines = false;
        }
        if has_lines {
            snippet.push('\n');
        } else {
            start = line_number;
        }
        snippet.push_str(line);
        end = line_number;
        has_lines = true;
    }

    if has_lines {
        result.push(CodeWindow {
            start,
            end,
            snippet,
        });
    }
}

/// Split `text` into chunks of at most `max_bytes` without breaking UTF-8
/// character boundaries.
pub fn utf8_chunks(mut text: &str, max_bytes: usize) -> Vec<&str> {
    let mut chunks = Vec::new();
    while !text.is_empty() {
        let mut end = text.len().min(max_bytes);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        chunks.push(&text[..end]);
        text = &text[end..];
    }
    chunks
}

/// Group windows into request-sized batches bounded by both window count and
/// total source bytes.
pub fn window_batches(windows: &[CodeWindow]) -> Vec<&[CodeWindow]> {
    let mut batches = Vec::new();
    let mut start = 0;
    let mut bytes = 0;
    for (index, window) in windows.iter().enumerate() {
        if index > start
            && (index - start >= MAX_WINDOWS_PER_REQUEST
                || bytes + window.snippet.len() > MAX_REQUEST_SOURCE_BYTES)
        {
            batches.push(&windows[start..index]);
            start = index;
            bytes = 0;
        }
        bytes += window.snippet.len();
    }
    if start < windows.len() {
        batches.push(&windows[start..]);
    }
    batches
}
