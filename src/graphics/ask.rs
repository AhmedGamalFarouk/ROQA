//! The permission question on the notice row: its two-line wrap, and whether it shows whole.

use super::*;

/// Whether a permission question shows whole in the notice's two lines at the narrowest text column (both buttons),
/// as the hook sees it: 96 DPI, with 2 pixels to spare for the rounding of the real pixel sizes.
pub fn ask_fits(text: &str) -> bool {
    ask_fits_at(text, 96, 2.0)
}

/// Whether the question shows whole on a notch of `dpi`, measured as `notice_text` draws it, with `spare` pixels left
/// over. A measure that fails counts as not whole.
pub fn ask_fits_at(text: &str, dpi: u32, spare: f32) -> bool {
    let scale = dpi as f32 / 96.0;
    let slot = layout::power_notice(false, dpi);
    let width = (layout::notice_button(false, dpi, 1).x - slot.x) as f32 - 6.0 * scale - spare;
    let font = Font {
        family: TEXT_FONT,
        size: 11.0 * scale,
        weight: DWRITE_FONT_WEIGHT_NORMAL,
        designator: 1.0,
        align: DWRITE_TEXT_ALIGNMENT_CENTER,
    };
    let lines = two_lines(text, |line| {
        text_width(line, &font).is_ok_and(|w| w <= width)
    });
    !lines.iter().any(|line| line.contains('\u{2026}'))
}

thread_local! {
    /// The wraps worked out for the question on show, by column width: it and the column stay the same for every
    /// frame it animates, and a new question drops the old ones.
    static WRAPPED: RefCell<Vec<(String, u32, [String; 2])>> = const { RefCell::new(Vec::new()) };
}

/// `two_lines`, worked out once per question and column width instead of on every frame.
pub(super) fn wrapped(text: &str, width: f32, fits: impl Fn(&str) -> bool) -> [String; 2] {
    WRAPPED.with_borrow_mut(|cache| {
        let key = width.to_bits();
        if let Some((.., lines)) = cache.iter().find(|(seen, w, _)| seen == text && *w == key) {
            return lines.clone();
        }
        if cache.first().is_some_and(|(seen, ..)| seen != text) {
            cache.clear();
        }
        let lines = two_lines(text, fits);
        cache.push((text.to_owned(), key, lines.clone()));
        lines
    })
}

/// `text` on at most two lines, `fits` telling whether a string is one line wide: the first line breaks after a space
/// when it has one, and a text too long for two keeps its start and its end around an ellipsis in the middle.
pub(super) fn two_lines(text: &str, fits: impl Fn(&str) -> bool) -> [String; 2] {
    let chars: Vec<char> = text.chars().collect();
    let wrap = |chars: &[char]| {
        let (mut low, mut high) = (0, chars.len());
        while low < high {
            let mid = (low + high).div_ceil(2);
            if fits(&chars[..mid].iter().collect::<String>()) {
                low = mid;
            } else {
                high = mid - 1;
            }
        }
        let cut = if low == chars.len() {
            low
        } else {
            let space = chars[..low].iter().rposition(|&c| c == ' ');
            space.map_or(low.max(1), |at| at + 1)
        };
        let rest: String = chars[cut..].iter().collect();
        fits(&rest).then(|| {
            [
                chars[..cut]
                    .iter()
                    .collect::<String>()
                    .trim_end()
                    .to_owned(),
                rest,
            ]
        })
    };
    if let Some(lines) = wrap(&chars) {
        return lines;
    }
    let around = |k: usize| -> Vec<char> {
        let end = chars.len() - k;
        chars[..k]
            .iter()
            .chain(&['\u{2026}'])
            .chain(&chars[end..])
            .copied()
            .collect()
    };
    let (mut low, mut high) = (1, (chars.len() / 2).max(1));
    while low < high {
        let mid = (low + high).div_ceil(2);
        if wrap(&around(mid)).is_some() {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    wrap(&around(low)).unwrap_or_else(|| [around(low).into_iter().collect(), String::new()])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_question_shows_in_full_on_two_lines_then_cuts_its_middle() {
        let wide = |line: &str| line.chars().count() <= 12;
        assert_eq!(two_lines("git status", wide), ["git status", ""]);
        // Two lines break after a space, and nothing is lost while it fits.
        assert_eq!(
            two_lines("Bash: git push origin", wide),
            ["Bash: git", "push origin"]
        );
        // Too long: the start and the end stay around one ellipsis, and both lines fit.
        let long = "Bash: rm -rf build target && cargo build --release 2>&1 | tail -n 5";
        let [first, second] = two_lines(long, wide);
        let joined = format!("{first} {second}");
        assert!(wide(&first) && wide(&second), "{first}|{second}");
        assert!(joined.starts_with("Bash:") && joined.ends_with("-n 5"));
        assert_eq!(joined.matches('\u{2026}').count(), 1);
        // One unbroken word has no space to break at.
        let [first, second] = two_lines("abcdefghijklmnopqrstuvwxyz", wide);
        assert!(wide(&first) && wide(&second) && first.starts_with("abc"));
    }

    #[test]
    fn a_wrap_is_worked_out_once_per_question_and_width() {
        let probes = std::cell::Cell::new(0);
        let fits = |line: &str| {
            probes.set(probes.get() + 1);
            line.chars().count() <= 12
        };
        let first = wrapped("Bash: git push origin", 100.0, fits);
        let used = probes.get();
        assert!(used > 0);
        assert_eq!(wrapped("Bash: git push origin", 100.0, fits), first);
        assert_eq!(probes.get(), used, "the second frame measures nothing");
        wrapped("Bash: git push origin", 120.0, fits);
        assert!(probes.get() > used, "another width is another wrap");
    }

    #[test]
    fn a_wide_command_is_not_shown_whole() {
        assert!(ask_fits("Bash: git push origin main"));
        // 59 characters, under the old count limit, but too wide for two lines of the column.
        assert!(!ask_fits(&format!("Bash: rm {}", "W".repeat(50))));
    }
}
