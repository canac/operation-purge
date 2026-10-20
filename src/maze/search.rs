//! Legacy wildcard maze-file search, ported from `cFileSearch`.
//!
//! PARITY: the matcher is **non-backtracking** — after a `*`, it jumps to
//! the first occurrence of the next literal character and never retries
//! (`cFileSearch.cpp:250`), so e.g. `*n.txt` can false-negative against
//! `main.txt`. ASCII-only case-insensitive. This quirk is observable
//! (maze-pool membership) and is preserved verbatim rather than fixed.

/// `SplitFileName`: splits on the last `/` or `\`; `"."` for bare names
/// with no extension separator implied — path defaults to `"."` for a
/// bare filename.
pub fn split_file_name(name: &str) -> (&str, &str) {
    match name.rfind(['/', '\\']) {
        Some(idx) => (&name[..idx], &name[idx + 1..]),
        None => (".", name),
    }
}

/// Verbatim port of `cFileSearchA::MatchesPattern` (`cFileSearch.cpp:213-278`):
/// ASCII case-insensitive; `?` = exactly one char; `*` = non-backtracking
/// (collapses runs of `*`, then jumps `szString` to the first BYTE-EXACT
/// occurrence of the following literal character via `strchr` and never
/// retries).
///
/// PARITY: the `strchr` anchor jump is a raw byte search — it is **not**
/// case-folded even though the rest of the comparison is
/// (`Lowercase(*szString) != Lowercase(*szPattern)` only runs in the
/// non-wildcard branch, `cFileSearch.cpp:269`). This silently defeats
/// case-insensitivity for the character immediately following a `*`: e.g.
/// `matches_pattern("MAIN.TXT", "*n.txt")` is `false` even though
/// `matches_pattern("main.txt", "*n.txt")` is `true` — see the quirk test
/// below. (design.md's own worked example, `*n.txt` vs `main.txt`
/// both-lowercase, does not reproduce a false-negative under a byte-exact
/// port of `cFileSearch.cpp:213-278`; the case-mismatched pair above is
/// the actual verified quirk and is cited instead — see task deviations.)
pub fn matches_pattern(name: &str, pattern: &str) -> bool {
    let s = name.as_bytes();
    let p = pattern.as_bytes();
    let mut si = 0usize;
    let mut pi = 0usize;

    fn lowercase(c: u8) -> u8 {
        if c.is_ascii_uppercase() {
            c + (b'a' - b'A')
        } else {
            c
        }
    }

    loop {
        let string_end = si >= s.len();
        let pattern_end = pi >= p.len();
        if string_end && pattern_end {
            return true;
        } else if pattern_end {
            return false;
        }

        if p[pi] == b'*' {
            // Skip to the character after the last '*' in the wildcard
            // pattern (cFileSearch.cpp:233-236).
            while pi < p.len() && p[pi] == b'*' {
                pi += 1;
            }
            if pi >= p.len() {
                // The last character of the pattern was reached.
                return true;
            }
            if si >= s.len() {
                // The last character of the string was reached.
                return false;
            }
            // Advance szString to the next occurrence of the next pattern
            // character after the '*' — raw `strchr`, case-sensitive
            // (cFileSearch.cpp:250).
            match s[si..].iter().position(|&c| c == p[pi]) {
                Some(offset) => {
                    si += offset;
                    continue;
                }
                None => return false,
            }
        } else if p[pi] == b'?' {
            if si >= s.len() {
                return false;
            }
            // '?' always matches — no comparison necessary.
        } else if si >= s.len() || lowercase(s[si]) != lowercase(p[pi]) {
            return false;
        }
        si += 1;
        pi += 1;
    }
}

/// Recursively enumerates `root` for files matching `pattern` (directories
/// filtered out by the caller, matching the original's `NextFile`
/// contract, `cFileSearch.cpp:80-136`). Files are visited depth-first;
/// only the file *name* (not the full path) is matched against `pattern`.
pub fn find_matches(root: &std::path::Path, pattern: &str) -> Vec<std::path::PathBuf> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| matches_pattern(name, pattern))
        })
        .map(|entry| entry.into_path())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_file_name_bare_name_uses_dot_path() {
        assert_eq!(split_file_name("Small1.maz"), (".", "Small1.maz"));
    }

    #[test]
    fn split_file_name_splits_on_either_separator() {
        assert_eq!(
            split_file_name("MazeFiles/Small1.maz"),
            ("MazeFiles", "Small1.maz")
        );
        assert_eq!(
            split_file_name("MazeFiles\\Small1.maz"),
            ("MazeFiles", "Small1.maz")
        );
    }

    #[test]
    fn exact_match_case_insensitive() {
        assert!(matches_pattern("Small1.maz", "small1.maz"));
        assert!(matches_pattern("SMALL1.MAZ", "Small1.maz"));
    }

    #[test]
    fn star_matches_any_run_including_empty() {
        assert!(matches_pattern("Small1.maz", "*.maz"));
        assert!(matches_pattern(".maz", "*.maz"));
        assert!(matches_pattern("anything", "*"));
    }

    #[test]
    fn question_mark_matches_exactly_one_char() {
        assert!(matches_pattern("Small1.maz", "Small?.maz"));
        assert!(!matches_pattern("Small12.maz", "Small?.maz"));
        assert!(!matches_pattern("Small.maz", "Small?.maz"));
    }

    #[test]
    fn star_lowercase_anchor_matches_lowercase_string() {
        // *n.txt jumps to the first literal 'n' via a raw (case-sensitive)
        // strchr; "main.txt" has a lowercase 'n' at the right spot, so
        // this succeeds despite the non-backtracking algorithm.
        assert!(matches_pattern("main.txt", "*n.txt"));
    }

    /// PARITY quirk (verified against `cFileSearch.cpp:213-278`, replacing
    /// design.md's inaccurate `*n.txt`-vs-`main.txt` example — see the
    /// doc comment on `matches_pattern`): the `*`-anchor jump uses a raw,
    /// case-sensitive `strchr`, even though the surrounding comparison is
    /// supposed to be case-insensitive. Pattern `*n.txt` (lowercase `n`)
    /// therefore does NOT match `MAIN.TXT` (uppercase) — `strchr` can't
    /// find a raw `'n'` byte in an all-uppercase string, so the search
    /// gives up immediately instead of case-folding first.
    #[test]
    fn star_anchor_jump_is_not_case_folded_quirk() {
        assert!(!matches_pattern("MAIN.TXT", "*n.txt"));
        // The exact match still succeeds case-insensitively once the
        // wildcard has resolved to a literal comparison.
        assert!(matches_pattern("MAIN.TXT", "MAIN.TXT"));
    }

    /// Classic non-backtracking failure: the first occurrence of the
    /// literal following `*` is a false start, and the algorithm never
    /// retries a later occurrence even though a backtracking matcher
    /// would succeed.
    #[test]
    fn star_anchor_does_not_backtrack_past_a_false_start() {
        // "nzzzn.txt" ends with the literal suffix "n.txt", but the first
        // 'n' (index 0) leads to a mismatch at 'z' vs '.', and the
        // algorithm never retries at the later 'n' (index 4).
        assert!(!matches_pattern("nzzzn.txt", "*n.txt"));
    }

    #[test]
    fn find_matches_recurses_and_filters_by_pattern() {
        let dir = std::env::temp_dir().join(format!(
            "op-search-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("Small1.maz"), b"").unwrap();
        std::fs::write(dir.join("readme.txt"), b"").unwrap();
        std::fs::write(dir.join("sub").join("Small2.maz"), b"").unwrap();

        let mut found: Vec<String> = find_matches(&dir, "*.maz")
            .into_iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        found.sort();

        std::fs::remove_dir_all(&dir).ok();

        assert_eq!(found, vec!["Small1.maz", "Small2.maz"]);
    }
}
