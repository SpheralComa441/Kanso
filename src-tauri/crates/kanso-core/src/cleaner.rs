/// Suffix phrases stripped (case-insensitively) from detected game titles
/// so metadata searches on SteamGridDB and Steam Store API match cleanly.
/// Ordered longest-first so compound suffixes match before shorter ones.
const EDITION_SUFFIXES: &[&str] = &[
    "game of the year edition",
    "game of the year",
    "goty edition",
    "goty",
    "director's cut",
    "directors cut",
    "definitive edition",
    "remastered edition",
    "remastered",
    "complete edition",
    "ultimate edition",
    "enhanced edition",
    "anniversary edition",
    "digital deluxe edition",
    "deluxe edition",
];

/// Cleans a detected game title by removing trademark symbols (`™`, `®`, `©`)
/// and common edition suffixes (`GOTY`, `Game of the Year`, `Definitive Edition`,
/// `Remastered`, `Director's Cut`, etc.).
pub fn clean_game_title(raw: &str) -> String {
    // 1. Strip trademark / copyright symbols and literal (TM) / (R) markers
    let mut working = raw
        .replace(['™', '®', '©'], "")
        .replace("(TM)", "")
        .replace("(tm)", "")
        .replace("(R)", "")
        .replace("(r)", "");

    working = collapse_whitespace(&working);

    // 2. Iteratively strip trailing parenthesized or separator-prefixed edition suffixes
    loop {
        let trimmed = trim_trailing_separators(&working);
        let Some(next) = strip_one_trailing_suffix(&trimmed) else {
            working = trimmed;
            break;
        };
        let cleaned_next = trim_trailing_separators(&collapse_whitespace(&next));
        if cleaned_next.is_empty() || cleaned_next == working {
            break;
        }
        working = cleaned_next;
    }

    let result = collapse_whitespace(&working);
    if result.is_empty() {
        collapse_whitespace(&raw.replace(['™', '®', '©'], ""))
    } else {
        result
    }
}

fn strip_one_trailing_suffix(input: &str) -> Option<String> {
    let lower = input.to_lowercase();

    for suffix in EDITION_SUFFIXES {
        // Case A: Suffix wrapped in parentheses or brackets at the end, e.g. "Title (GOTY)"
        for (open, close) in [('(', ')'), ('[', ']')] {
            let wrapped = format!("{open}{suffix}{close}");
            if lower.ends_with(&wrapped) {
                let cut_at = input.len() - wrapped.len();
                return Some(input[..cut_at].to_string());
            }
        }

        // Case B: Plain suffix at the end preceded by whitespace or separator
        if lower.ends_with(suffix) && input.len() > suffix.len() {
            let cut_at = input.len() - suffix.len();
            let prefix = &input[..cut_at];
            if prefix
                .chars()
                .last()
                .is_some_and(|ch| ch.is_whitespace() || matches!(ch, '-' | ':' | '–' | '—' | '|'))
            {
                return Some(prefix.to_string());
            }
        }
    }

    None
}

fn trim_trailing_separators(s: &str) -> String {
    s.trim_end_matches(|ch: char| ch.is_whitespace() || matches!(ch, '-' | ':' | '–' | '—' | '|'))
        .trim_start()
        .to_string()
}

fn collapse_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_trademarks_and_edition_suffixes() {
        assert_eq!(
            clean_game_title("The Witcher® 3: Wild Hunt - Game of the Year Edition"),
            "The Witcher 3: Wild Hunt"
        );
        assert_eq!(
            clean_game_title("Batman™: Arkham Asylum GOTY Edition"),
            "Batman: Arkham Asylum"
        );
        assert_eq!(
            clean_game_title("DEATH STRANDING DIRECTOR'S CUT"),
            "DEATH STRANDING"
        );
        assert_eq!(
            clean_game_title("Marvel's Spider-Man Remastered"),
            "Marvel's Spider-Man"
        );
        assert_eq!(
            clean_game_title("Divinity: Original Sin 2 - Definitive Edition"),
            "Divinity: Original Sin 2"
        );
        assert_eq!(
            clean_game_title("Sekiro™: Shadows Die Twice (GOTY)"),
            "Sekiro: Shadows Die Twice"
        );
    }

    #[test]
    fn preserves_regular_titles() {
        assert_eq!(clean_game_title("Hollow Knight"), "Hollow Knight");
        assert_eq!(clean_game_title("Cyberpunk 2077"), "Cyberpunk 2077");
    }
}
