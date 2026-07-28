//! In-app spell-checking for the built-in Markdown editor, backed by a
//! bundled English Hunspell dictionary via `spellbook` — pure Rust, no
//! system dependency, no network, so it works the same in every feature
//! set. See `assets/dictionaries/en` for the dictionary's own provenance
//! and licence.

use std::sync::OnceLock;

const AFF: &str = include_str!("../assets/dictionaries/en/index.aff");
const DIC: &str = include_str!("../assets/dictionaries/en/index.dic");

fn dictionary() -> &'static spellbook::Dictionary {
    static DICTIONARY: OnceLock<spellbook::Dictionary> = OnceLock::new();
    DICTIONARY.get_or_init(|| {
        spellbook::Dictionary::new(AFF, DIC).expect("bundled dictionary should always parse")
    })
}

/// Whether `c` can appear inside a word — letters/digits, plus the interior
/// punctuation contractions and hyphenated words use.
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '\'' || c == '\u{2019}' || c == '-'
}

/// Byte ranges of maximal `is_word_char` runs in `line`.
fn raw_word_spans(line: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start: Option<usize> = None;
    for (i, c) in line.char_indices() {
        if is_word_char(c) {
            start.get_or_insert(i);
        } else if let Some(s) = start.take() {
            spans.push((s, i));
        }
    }
    if let Some(s) = start {
        spans.push((s, line.len()));
    }
    spans
}

/// Trims leading/trailing apostrophes and hyphens off a raw word span (so a
/// 'quoted' word doesn't include the quotes, and a run of just punctuation
/// isn't treated as a word at all), and drops spans with no letter in them
/// — plain numbers aren't a spelling concern.
fn trim_word_span(line: &str, start: usize, end: usize) -> Option<(usize, usize)> {
    const TRIM: [char; 3] = ['\'', '\u{2019}', '-'];
    let slice = &line[start..end];
    let trimmed = slice.trim_matches(TRIM.as_slice());
    if trimmed.is_empty() || !trimmed.chars().any(char::is_alphabetic) {
        return None;
    }
    let leading = slice.len() - slice.trim_start_matches(TRIM.as_slice()).len();
    let new_start = start + leading;
    Some((new_start, new_start + trimmed.len()))
}

/// Byte ranges of misspelled words within a single line, excluding any text
/// inside inline Markdown code spans (`` `...` ``) — code isn't prose.
pub fn misspelled_spans(line: &str) -> Vec<(usize, usize)> {
    let code_ranges = inline_code_ranges(line);
    raw_word_spans(line)
        .into_iter()
        .filter_map(|(s, e)| trim_word_span(line, s, e))
        .filter(|(s, e)| !code_ranges.iter().any(|(cs, ce)| *cs <= *s && *e <= *ce))
        .filter(|(s, e)| !dictionary().check(&line[*s..*e]))
        .collect()
}

/// Byte ranges covered by inline code spans in `line`, including their
/// backtick delimiters. Follows Markdown's actual code-span rule: a run of
/// N backticks opens a span, closed only by the next run of exactly N
/// backticks (so `` `` `` a run of 2 skips right over a lone backtick
/// inside it, rather than closing early) — not just "next backtick closes
/// it", which would wrongly split a `` ``escaped ` backtick`` `` span in
/// two. An unterminated opening run is treated as running to the end of
/// the line, matching how it renders.
fn inline_code_ranges(line: &str) -> Vec<(usize, usize)> {
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    let mut ranges = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].1 != '`' {
            i += 1;
            continue;
        }
        let open_start = chars[i].0;
        let open_len = backtick_run_len(&chars, i);
        let mut j = i + open_len;
        let close = loop {
            if j >= chars.len() {
                break None;
            }
            if chars[j].1 != '`' {
                j += 1;
                continue;
            }
            let run_len = backtick_run_len(&chars, j);
            if run_len == open_len {
                break Some(j + run_len);
            }
            j += run_len;
        };
        match close {
            Some(end_idx) => {
                let end_byte = chars.get(end_idx).map_or(line.len(), |(b, _)| *b);
                ranges.push((open_start, end_byte));
                i = end_idx;
            }
            None => {
                ranges.push((open_start, line.len()));
                break;
            }
        }
    }
    ranges
}

/// The length of the run of consecutive backticks starting at `chars[i]`
/// (which must itself be a backtick).
fn backtick_run_len(chars: &[(usize, char)], i: usize) -> usize {
    chars[i..].iter().take_while(|(_, c)| *c == '`').count()
}

/// Misspelled-word byte ranges for every line in a buffer, skipping any
/// line inside a fenced code block (a line whose trimmed start is ` ``` `)
/// entirely.
pub fn misspelled_spans_in_buffer(lines: &[String]) -> Vec<Vec<(usize, usize)>> {
    misspelled_spans_in_range(lines, 0, lines.len())
}

/// Like `misspelled_spans_in_buffer`, but only checks `lines[start..start +
/// count]` against the dictionary — the fence state is still derived from
/// the whole buffer up to `start` first (a cheap fence-marker count, no
/// dictionary lookups), so a fence that began off-screen is still respected
/// once it scrolls into view. Lets the editor's renderer avoid re-checking
/// every off-screen line against the dictionary on every frame.
pub fn misspelled_spans_in_range(
    lines: &[String],
    start: usize,
    count: usize,
) -> Vec<Vec<(usize, usize)>> {
    let mut in_fence = lines[..start.min(lines.len())]
        .iter()
        .filter(|l| is_fence_marker(l))
        .count()
        % 2
        == 1;
    lines
        .iter()
        .skip(start)
        .take(count)
        .map(|line| {
            if is_fence_marker(line) {
                in_fence = !in_fence;
                return Vec::new();
            }
            if in_fence {
                return Vec::new();
            }
            misspelled_spans(line)
        })
        .collect()
}

fn is_fence_marker(line: &str) -> bool {
    line.trim_start().starts_with("```")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_misspelled_words_with_correct_byte_ranges() {
        let line = "This sentnce has a typo.";
        let start = line.find("sentnce").unwrap();
        let end = start + "sentnce".len();
        assert_eq!(misspelled_spans(line), vec![(start, end)]);
    }

    #[test]
    fn does_not_flag_correctly_spelled_prose() {
        assert_eq!(misspelled_spans("This sentence is fine."), Vec::new());
    }

    #[test]
    fn recognizes_contractions_and_hyphenated_words() {
        assert_eq!(misspelled_spans("It's a well-known fact."), Vec::new());
    }

    #[test]
    fn skips_plain_numbers() {
        assert_eq!(misspelled_spans("There are 12345 of them."), Vec::new());
    }

    #[test]
    fn skips_inline_code_spans() {
        // `mispeled` inside backticks must not be flagged, but the same
        // word outside backticks on the same line must be.
        let line = "Run `mispeled()` then check mispeled.";
        let spans = misspelled_spans(line);
        let expected_start = line.rfind("mispeled").unwrap();
        assert_eq!(spans, vec![(expected_start, expected_start + 8)]);
    }

    #[test]
    fn skips_fenced_code_blocks_entirely() {
        let lines: Vec<String> = vec![
            "Before the fence has a typo: sentnce.".into(),
            "```".into(),
            "let mispeled = 1;".into(),
            "```".into(),
            "After the fence has a typo too: sentnce.".into(),
        ];
        let spans = misspelled_spans_in_buffer(&lines);
        assert_eq!(spans.len(), 5);
        assert_eq!(spans[0].len(), 1, "prose before the fence is checked");
        assert!(spans[1].is_empty(), "the fence marker itself is skipped");
        assert!(spans[2].is_empty(), "code inside the fence is skipped");
        assert!(spans[3].is_empty(), "the closing fence marker is skipped");
        assert_eq!(spans[4].len(), 1, "prose after the fence is checked");
    }

    #[test]
    fn an_unterminated_trailing_backtick_runs_to_the_end_of_the_line() {
        // A single stray backtick shouldn't panic or leave the rest of the
        // line unchecked in some inconsistent way — everything from the
        // backtick onward is conservatively treated as code, but the typo
        // before it must still be flagged.
        let line = "This has a typo: sentnce ` and a stray backtick";
        let start = line.find("sentnce").unwrap();
        assert_eq!(
            misspelled_spans(line),
            vec![(start, start + "sentnce".len())]
        );
    }

    #[test]
    fn a_quoted_word_does_not_include_the_quotes_in_its_span() {
        let line = "She said 'hello' to everyone.";
        // 'hello' is correctly spelled once the quotes are trimmed, so
        // nothing should be flagged — a bug here would either flag
        // "'hello'" (quotes included) as misspelled, or fail to trim at all.
        assert_eq!(misspelled_spans(line), Vec::new());
    }

    #[test]
    fn double_backtick_code_spans_are_skipped_even_with_a_lone_backtick_inside() {
        // A `` ``two-backtick`` `` span is how Markdown escapes a literal
        // backtick inside code — it must close on the next *matching*
        // 2-backtick run, not on the first single backtick it meets.
        let line = "before ``mispeled`` after, and `` a ` backtick `` too";
        assert_eq!(
            misspelled_spans(line),
            Vec::new(),
            "both double-backtick spans should be fully skipped"
        );
    }

    #[test]
    fn misspelled_spans_in_range_respects_a_fence_that_started_before_the_window() {
        let lines: Vec<String> = vec![
            "```".into(),
            "let mispeled = 1;".into(),
            "still code, sentnce typo".into(),
            "```".into(),
            "prose after the fence: sentnce".into(),
        ];
        // Window starts mid-fence (line 2) — must still know it's in a
        // fence without re-scanning line 0's dictionary lookups.
        let spans = misspelled_spans_in_range(&lines, 2, 3);
        assert_eq!(spans.len(), 3);
        assert!(spans[0].is_empty(), "still inside the fence");
        assert!(spans[1].is_empty(), "the closing fence marker is skipped");
        assert_eq!(spans[2].len(), 1, "prose after the fence is checked");
    }
}
