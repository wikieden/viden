use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

const HORIZONTAL: char = '─';

#[allow(dead_code)]
pub(super) fn top_border(width: usize) -> String {
    full_border(width, '┌', '┐')
}

pub(super) fn bottom_border(width: usize) -> String {
    full_border(width, '└', '┘')
}

pub(super) fn horizontal(width: usize) -> String {
    HORIZONTAL.to_string().repeat(width)
}

fn full_border(width: usize, left: char, right: char) -> String {
    format!("{left}{}{right}", horizontal(width.saturating_sub(2)))
}

pub(super) fn char_width(value: &str) -> usize {
    UnicodeWidthStr::width(value)
}

pub(super) fn pad(value: &str, width: usize) -> String {
    let value = truncate(value, width);
    let padding = width.saturating_sub(char_width(&value));
    format!("{value}{}", " ".repeat(padding))
}

pub(super) fn truncate(value: &str, width: usize) -> String {
    let mut used = 0usize;
    let mut output = String::new();
    for value in value.graphemes(true) {
        let next = UnicodeWidthStr::width(value);
        if used + next > width {
            break;
        }
        output.push_str(value);
        used += next;
    }
    output
}

/// Truncates keeping the DISTINCTIVE END of a label, marking the cut with `…`.
///
/// Head truncation collides whenever items differ only after the cut: two
/// `.github/workflows/*` paths both render as `.github/workflow`, and gate ids
/// sharing a long `gate-acp-session-…` prefix all render as `Merge gate gate-`.
/// Keeping the tail instead preserves exactly the part that distinguishes
/// them — a path's filename and an id's random suffix — so two different items
/// never render as the same row while the width allows any distinction.
///
/// The suffix is taken by display width rather than by path component: snapping
/// to a `/` boundary would drop the parent directory, which is the only thing
/// separating two files that share a filename.
pub(super) fn truncate_tail(value: &str, width: usize) -> String {
    if char_width(value) <= width {
        return value.to_string();
    }
    // Below two columns there is no room for both the marker and any content,
    // so fall back to a plain head cut rather than rendering just a marker.
    if width < 2 {
        return truncate(value, width);
    }
    format!("…{}", suffix_by_width(value, width - 1))
}

#[allow(dead_code)]
pub(super) fn compact_middle(value: &str, width: usize) -> String {
    if char_width(value) <= width {
        return value.to_string();
    }
    if width <= 3 {
        return truncate(value, width);
    }
    let head = (width - 1) / 2;
    let tail = width - head - 1;
    let prefix = truncate(value, head);
    let suffix = suffix_by_width(value, tail);
    format!("{prefix}~{suffix}")
}

pub(super) fn wrap_words(content: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![String::new()];
    }
    if char_width(content) <= width {
        return vec![content.to_string()];
    }

    let mut rows = Vec::new();
    let mut current = String::new();
    for word in content.split_whitespace() {
        if char_width(word) > width {
            if !current.is_empty() {
                rows.push(current);
                current = String::new();
            }
            push_wrapped_token(&mut rows, &mut current, word, width);
            continue;
        }

        let pending_width = if current.is_empty() {
            char_width(word)
        } else {
            char_width(&current) + 1 + char_width(word)
        };
        if pending_width > width && !current.is_empty() {
            rows.push(current);
            current = word.to_string();
        } else if current.is_empty() {
            current = word.to_string();
        } else {
            current.push(' ');
            current.push_str(word);
        }
    }
    if !current.is_empty() {
        rows.push(current);
    }
    if rows.is_empty() {
        rows.push(String::new());
    }
    rows
}

fn push_wrapped_token(rows: &mut Vec<String>, current: &mut String, token: &str, width: usize) {
    let mut used = 0usize;
    for grapheme in token.graphemes(true) {
        let grapheme_width = UnicodeWidthStr::width(grapheme);
        if grapheme_width == 0 {
            current.push_str(grapheme);
            continue;
        }
        if used + grapheme_width > width && !current.is_empty() {
            rows.push(std::mem::take(current));
            used = 0;
        }
        if grapheme_width > width {
            continue;
        }
        current.push_str(grapheme);
        used += grapheme_width;
    }
    if used == width && !current.is_empty() {
        rows.push(std::mem::take(current));
    }
}

#[allow(dead_code)]
fn suffix_by_width(value: &str, width: usize) -> String {
    let mut used = 0usize;
    let mut graphemes = Vec::new();
    for grapheme in value.graphemes(true).rev() {
        let next = UnicodeWidthStr::width(grapheme);
        if used + next > width {
            break;
        }
        graphemes.push(grapheme);
        used += next;
    }
    graphemes.into_iter().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cjk_chars_count_as_double_width() {
        assert_eq!(char_width("abc"), 3);
        assert_eq!(char_width("你好"), 4);
        assert_eq!(char_width("a你b"), 4);
    }

    #[test]
    fn pad_and_truncate_use_display_width() {
        assert_eq!(char_width(&pad("你好", 6)), 6);
        assert_eq!(truncate("你好abc", 5), "你好a");
        assert_eq!(truncate("你好abc", 3), "你");
    }

    #[test]
    fn emoji_modifiers_and_combining_marks_do_not_add_cells() {
        assert_eq!(char_width("👋🏻"), 2);
        assert_eq!(char_width("a\u{0301}"), 1);
        assert_eq!(char_width("你\u{FE0F}"), 2);
    }

    #[test]
    fn wraps_by_display_width_not_codepoint_count() {
        let rows = wrap_words("你好你好你好", 4);

        assert_eq!(rows, vec!["你好", "你好", "你好"]);
        assert!(rows.iter().all(|row| char_width(row) <= 4));
    }

    #[test]
    fn compact_middle_uses_display_width() {
        let compacted = compact_middle("路径/你好世界/config.toml", 12);

        assert!(char_width(&compacted) <= 12);
        assert!(compacted.contains('~'));
    }
}

#[cfg(test)]
mod tail_truncation_tests {
    use super::*;

    /// The exact live collision: two workflow files rendering as one label.
    #[test]
    fn two_long_sibling_paths_never_render_the_same_label() {
        let left = ".github/workflows/ci.yml";
        let right = ".github/workflows/release.yml";
        assert_eq!(
            truncate(left, 16),
            truncate(right, 16),
            "head truncation collides"
        );
        assert_ne!(truncate_tail(left, 16), truncate_tail(right, 16));
        // The filename is the part that must survive.
        assert!(truncate_tail(left, 16).ends_with("ci.yml"));
        assert!(truncate_tail(right, 16).ends_with("release.yml"));
    }

    /// Gate ids differ only at the end, so a head cut renders them identically.
    #[test]
    fn two_same_prefix_gate_ids_never_render_the_same_label() {
        let left = "Merge gate gate-acp-session-019fb746-46bc-7641-91ff-ba2e4ac51cdc";
        let right = "Merge gate gate-acp-session-019fb769-b057-7861-b523-d2aff89ca6b8";
        assert_eq!(
            truncate(left, 16),
            truncate(right, 16),
            "head truncation collides"
        );
        assert_ne!(truncate_tail(left, 16), truncate_tail(right, 16));
        // At least the last twelve characters of an id survive.
        assert!(truncate_tail(left, 16).ends_with("ba2e4ac51cdc"));
        assert!(truncate_tail(right, 16).ends_with("d2aff89ca6b8"));
    }

    #[test]
    fn a_label_that_fits_is_returned_unchanged_without_a_marker() {
        assert_eq!(truncate_tail("short", 16), "short");
        assert_eq!(truncate_tail("exactly-sixteen!", 16), "exactly-sixteen!");
    }

    #[test]
    fn tail_truncation_never_exceeds_the_requested_width() {
        for width in 0..24 {
            let rendered = truncate_tail(".github/workflows/release.yml", width);
            assert!(char_width(&rendered) <= width, "width {width} overflowed");
        }
    }

    /// CJK graphemes are two columns wide; the cut must respect that.
    #[test]
    fn tail_truncation_respects_wide_graphemes() {
        let value = "报告/最终/结论.md";
        let rendered = truncate_tail(value, 10);
        assert!(char_width(&rendered) <= 10);
        assert!(rendered.starts_with('…'));
    }
}
