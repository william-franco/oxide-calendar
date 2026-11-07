use super::event::Event;

/// Folds a character to its lowercase base letter, so "São" matches "sao".
fn fold_char(c: char) -> char {
    match c {
        'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' => 'i',
        'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
        'ú' | 'ù' | 'û' | 'ü' => 'u',
        'ç' => 'c',
        other => other,
    }
}

pub fn normalize(text: &str) -> String {
    text.chars().flat_map(char::to_lowercase).map(fold_char).collect()
}

/// True when the query appears in the title, description or location, ignoring case and accents.
/// An empty query matches everything.
pub fn matches(event: &Event, query: &str) -> bool {
    let needle = normalize(query.trim());
    if needle.is_empty() {
        return true;
    }
    [&event.title, &event.description, &event.location]
        .iter()
        .any(|field| normalize(field).contains(&needle))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn event(title: &str, location: &str) -> Event {
        let start = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap().and_hms_opt(9, 0, 0).unwrap();
        Event {
            id: None,
            uid: "u".into(),
            title: title.into(),
            description: String::new(),
            location: location.into(),
            category_id: 1,
            all_day: false,
            start,
            end: start,
            rrule: None,
            reminder_minutes: None,
        }
    }

    #[test]
    fn ignores_case_and_accents() {
        let e = event("Reunião com São Paulo", "Sala 3");
        assert!(matches(&e, "reuniao"));
        assert!(matches(&e, "SAO"));
        assert!(matches(&e, "  sala "));
        assert!(!matches(&e, "dentista"));
    }

    #[test]
    fn empty_query_matches_all() {
        assert!(matches(&event("x", ""), "   "));
    }
}
