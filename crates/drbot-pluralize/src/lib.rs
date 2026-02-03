//! Pluralization utilities for drbot.
//!
//! This crate provides:
//! - English pluralization rules
//! - Custom plural rules
//! - Number formatting with plurals

use std::collections::HashMap;
use thiserror::Error;

/// Pluralize error types.
#[derive(Error, Debug)]
pub enum PluralizeError {
    #[error("Invalid word: {0}")]
    InvalidWord(String),
}

/// Result type for pluralize operations.
pub type Result<T> = std::result::Result<T, PluralizeError>;

/// Pluralizer with English rules.
pub struct Pluralizer {
    /// Irregular plurals (singular -> plural).
    irregulars: HashMap<String, String>,
    /// Uncountable words.
    uncountables: Vec<String>,
}

impl Pluralizer {
    /// Create new pluralizer with default rules.
    pub fn new() -> Self {
        let mut p = Self {
            irregulars: HashMap::new(),
            uncountables: Vec::new(),
        };
        p.add_default_rules();
        p
    }

    /// Create empty pluralizer.
    pub fn empty() -> Self {
        Self {
            irregulars: HashMap::new(),
            uncountables: Vec::new(),
        }
    }

    fn add_default_rules(&mut self) {
        // Irregular plurals
        let irregulars = [
            ("person", "people"),
            ("man", "men"),
            ("woman", "women"),
            ("child", "children"),
            ("tooth", "teeth"),
            ("foot", "feet"),
            ("goose", "geese"),
            ("mouse", "mice"),
            ("ox", "oxen"),
            ("leaf", "leaves"),
            ("life", "lives"),
            ("knife", "knives"),
            ("wife", "wives"),
            ("half", "halves"),
            ("self", "selves"),
            ("calf", "calves"),
            ("loaf", "loaves"),
            ("potato", "potatoes"),
            ("tomato", "tomatoes"),
            ("hero", "heroes"),
            ("echo", "echoes"),
            ("cargo", "cargoes"),
            ("criterion", "criteria"),
            ("phenomenon", "phenomena"),
            ("datum", "data"),
            ("analysis", "analyses"),
            ("basis", "bases"),
            ("crisis", "crises"),
            ("thesis", "theses"),
            ("hypothesis", "hypotheses"),
            ("axis", "axes"),
            ("appendix", "appendices"),
            ("index", "indices"),
            ("matrix", "matrices"),
            ("vertex", "vertices"),
            ("focus", "foci"),
            ("cactus", "cacti"),
            ("fungus", "fungi"),
            ("nucleus", "nuclei"),
            ("stimulus", "stimuli"),
            ("alumnus", "alumni"),
        ];

        for (singular, plural) in irregulars {
            self.irregulars
                .insert(singular.to_string(), plural.to_string());
        }

        // Uncountable words
        let uncountables = [
            "equipment",
            "information",
            "rice",
            "money",
            "species",
            "series",
            "fish",
            "sheep",
            "deer",
            "moose",
            "aircraft",
            "news",
            "advice",
            "furniture",
            "luggage",
            "traffic",
            "software",
            "hardware",
            "feedback",
            "knowledge",
            "research",
            "progress",
            "evidence",
        ];

        for word in uncountables {
            self.uncountables.push(word.to_string());
        }
    }

    /// Add irregular plural.
    pub fn add_irregular(&mut self, singular: &str, plural: &str) {
        self.irregulars
            .insert(singular.to_lowercase(), plural.to_lowercase());
    }

    /// Add uncountable word.
    pub fn add_uncountable(&mut self, word: &str) {
        self.uncountables.push(word.to_lowercase());
    }

    /// Check if word is uncountable.
    pub fn is_uncountable(&self, word: &str) -> bool {
        self.uncountables.contains(&word.to_lowercase())
    }

    /// Pluralize a word.
    pub fn pluralize(&self, word: &str) -> String {
        let lower = word.to_lowercase();

        // Check uncountable
        if self.is_uncountable(&lower) {
            return word.to_string();
        }

        // Check irregular
        if let Some(plural) = self.irregulars.get(&lower) {
            return self.match_case(word, plural);
        }

        // Apply regular rules
        let plural = self.apply_rules(&lower);
        self.match_case(word, &plural)
    }

    /// Singularize a word.
    pub fn singularize(&self, word: &str) -> String {
        let lower = word.to_lowercase();

        // Check uncountable
        if self.is_uncountable(&lower) {
            return word.to_string();
        }

        // Check irregular (reverse lookup)
        for (singular, plural) in &self.irregulars {
            if plural == &lower {
                return self.match_case(word, singular);
            }
        }

        // Apply reverse rules
        let singular = self.apply_singular_rules(&lower);
        self.match_case(word, &singular)
    }

    fn apply_rules(&self, word: &str) -> String {
        // Words ending in consonant + y -> ies
        if word.ends_with('y') && word.len() > 1 {
            let prev = word.chars().nth(word.len() - 2).unwrap();
            if !is_vowel(prev) {
                return format!("{}ies", &word[..word.len() - 1]);
            }
        }

        // Words ending in s, x, z, ch, sh -> es
        if word.ends_with('s')
            || word.ends_with('x')
            || word.ends_with('z')
            || word.ends_with("ch")
            || word.ends_with("sh")
        {
            return format!("{}es", word);
        }

        // Words ending in f or fe -> ves
        if word.ends_with('f') {
            return format!("{}ves", &word[..word.len() - 1]);
        }
        if word.ends_with("fe") {
            return format!("{}ves", &word[..word.len() - 2]);
        }

        // Words ending in o -> os or oes (default to os)
        if word.ends_with('o') {
            let prev = word.chars().nth(word.len() - 2);
            if prev.map(is_vowel).unwrap_or(false) {
                return format!("{}s", word);
            }
            // Most -o words just add s
            return format!("{}s", word);
        }

        // Default: add s
        format!("{}s", word)
    }

    fn apply_singular_rules(&self, word: &str) -> String {
        // Words ending in ies -> y
        if word.ends_with("ies") && word.len() > 3 {
            return format!("{}y", &word[..word.len() - 3]);
        }

        // Words ending in ves -> f or fe
        if word.ends_with("ves") && word.len() > 3 {
            let stem = &word[..word.len() - 3];
            // Try fe first for common words
            if stem.ends_with('l') || stem.ends_with('n') {
                return format!("{}fe", stem);
            }
            return format!("{}f", stem);
        }

        // Words ending in es
        if word.ends_with("es") && word.len() > 2 {
            let stem = &word[..word.len() - 2];
            // Check if it was s, x, z, ch, sh + es
            if stem.ends_with('s')
                || stem.ends_with('x')
                || stem.ends_with('z')
                || stem.ends_with("ch")
                || stem.ends_with("sh")
            {
                return stem.to_string();
            }
            // Otherwise just remove s
            return format!("{}e", stem);
        }

        // Words ending in s
        if word.ends_with('s') && word.len() > 1 {
            return word[..word.len() - 1].to_string();
        }

        word.to_string()
    }

    fn match_case(&self, original: &str, result: &str) -> String {
        if original.chars().all(|c| c.is_uppercase()) {
            result.to_uppercase()
        } else if original
            .chars()
            .next()
            .map(|c| c.is_uppercase())
            .unwrap_or(false)
        {
            let mut chars = result.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        } else {
            result.to_string()
        }
    }
}

impl Default for Pluralizer {
    fn default() -> Self {
        Self::new()
    }
}

fn is_vowel(c: char) -> bool {
    matches!(c.to_ascii_lowercase(), 'a' | 'e' | 'i' | 'o' | 'u')
}

/// Quick pluralize function.
pub fn pluralize(word: &str) -> String {
    Pluralizer::new().pluralize(word)
}

/// Quick singularize function.
pub fn singularize(word: &str) -> String {
    Pluralizer::new().singularize(word)
}

/// Pluralize with count.
pub fn pluralize_with_count(word: &str, count: i64) -> String {
    if count == 1 || count == -1 {
        format!("{} {}", count, word)
    } else {
        format!("{} {}", count, pluralize(word))
    }
}

/// Pluralize noun phrase (handles "1 item" vs "2 items").
pub fn count_noun(count: i64, singular: &str, plural: Option<&str>) -> String {
    let noun = if count == 1 || count == -1 {
        singular.to_string()
    } else {
        plural
            .map(|p| p.to_string())
            .unwrap_or_else(|| pluralize(singular))
    };
    format!("{} {}", count, noun)
}

/// Check if a word is likely plural.
pub fn is_plural(word: &str) -> bool {
    let lower = word.to_lowercase();
    let singular = singularize(&lower);
    let re_pluralized = pluralize(&singular);
    re_pluralized == lower && singular != lower
}

/// Check if a word is likely singular.
pub fn is_singular(word: &str) -> bool {
    let lower = word.to_lowercase();
    let pluralized = pluralize(&lower);
    pluralized != lower
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_regular_plurals() {
        assert_eq!(pluralize("cat"), "cats");
        assert_eq!(pluralize("dog"), "dogs");
        assert_eq!(pluralize("house"), "houses");
    }

    #[test]
    fn test_es_plurals() {
        assert_eq!(pluralize("bus"), "buses");
        assert_eq!(pluralize("box"), "boxes");
        assert_eq!(pluralize("buzz"), "buzzes");
        assert_eq!(pluralize("church"), "churches");
        assert_eq!(pluralize("dish"), "dishes");
    }

    #[test]
    fn test_y_plurals() {
        assert_eq!(pluralize("city"), "cities");
        assert_eq!(pluralize("baby"), "babies");
        assert_eq!(pluralize("day"), "days"); // vowel + y
        assert_eq!(pluralize("key"), "keys"); // vowel + y
    }

    #[test]
    fn test_irregular_plurals() {
        assert_eq!(pluralize("person"), "people");
        assert_eq!(pluralize("child"), "children");
        assert_eq!(pluralize("tooth"), "teeth");
        assert_eq!(pluralize("mouse"), "mice");
    }

    #[test]
    fn test_uncountable() {
        assert_eq!(pluralize("sheep"), "sheep");
        assert_eq!(pluralize("fish"), "fish");
        assert_eq!(pluralize("information"), "information");
    }

    #[test]
    fn test_case_preservation() {
        assert_eq!(pluralize("Cat"), "Cats");
        assert_eq!(pluralize("CAT"), "CATS");
        assert_eq!(pluralize("Person"), "People");
    }

    #[test]
    fn test_singularize() {
        assert_eq!(singularize("cats"), "cat");
        assert_eq!(singularize("cities"), "city");
        assert_eq!(singularize("people"), "person");
        assert_eq!(singularize("children"), "child");
    }

    #[test]
    fn test_pluralize_with_count() {
        assert_eq!(pluralize_with_count("item", 1), "1 item");
        assert_eq!(pluralize_with_count("item", 2), "2 items");
        assert_eq!(pluralize_with_count("item", 0), "0 items");
    }

    #[test]
    fn test_count_noun() {
        assert_eq!(count_noun(1, "person", Some("people")), "1 person");
        assert_eq!(count_noun(5, "person", Some("people")), "5 people");
        assert_eq!(count_noun(1, "cat", None), "1 cat");
        assert_eq!(count_noun(3, "cat", None), "3 cats");
    }

    #[test]
    fn test_is_plural() {
        assert!(is_plural("cats"));
        assert!(is_plural("cities"));
        assert!(!is_plural("cat"));
        assert!(!is_plural("city"));
    }

    #[test]
    fn test_custom_rules() {
        let mut p = Pluralizer::new();
        p.add_irregular("octopus", "octopi");
        assert_eq!(p.pluralize("octopus"), "octopi");
    }
}
