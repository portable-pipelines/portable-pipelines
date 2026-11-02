//! Per-stage stream identifier names (Author's Edition stream identifiers).
//!
//! A stream is still a [`StreamId`] number. A name is a word on one stage
//! that resolves to that number. Reserved words `PRIMARY`, `SECONDARY`, and
//! `TERTIARY` are 0, 1, and 2 on every stage. User names are case-insensitive
//! and live only on the stage that defined them.

use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::StreamId;

/// Errors when resolving or defining a stream identifier word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamIdentError {
    /// Word is not a number, not reserved, and not defined on this stage.
    Unknown(String),
    /// The same user name is defined twice on this stage.
    Duplicate(String),
    /// `PRIMARY` / `SECONDARY` / `TERTIARY` cannot be assigned a new number.
    Reserved(String),
}

impl fmt::Display for StreamIdentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown(w) => write!(f, "unknown stream identifier {w}"),
            Self::Duplicate(w) => write!(f, "duplicate stream identifier {w}"),
            Self::Reserved(w) => write!(f, "cannot rename reserved stream identifier {w}"),
        }
    }
}

impl std::error::Error for StreamIdentError {}

/// Name-to-number map for one stage.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StreamNames {
    /// Keys are stored in ASCII uppercase.
    map: HashMap<String, u32>,
}

impl StreamNames {
    pub fn new() -> Self {
        Self::default()
    }

    /// Resolve a number, reserved word, or previously defined name.
    ///
    /// Does not define a new name.
    pub fn resolve(&self, token: &str) -> Result<StreamId, StreamIdentError> {
        let t = token.trim();
        if t.is_empty() {
            return Err(StreamIdentError::Unknown(token.to_string()));
        }
        if let Ok(n) = t.parse::<u32>() {
            return Ok(StreamId(n));
        }
        let key = t.to_ascii_uppercase();
        if let Some(id) = reserved_stream(&key) {
            return Ok(StreamId(id));
        }
        match self.map.get(&key) {
            Some(&id) => Ok(StreamId(id)),
            None => Err(StreamIdentError::Unknown(t.to_string())),
        }
    }

    /// Bind `name` to `id`. Fails on a reserved word or a second definition.
    pub fn define(&mut self, name: &str, id: StreamId) -> Result<(), StreamIdentError> {
        let t = name.trim();
        let key = t.to_ascii_uppercase();
        if reserved_stream(&key).is_some() {
            return Err(StreamIdentError::Reserved(t.to_string()));
        }
        if self.map.contains_key(&key) {
            return Err(StreamIdentError::Duplicate(t.to_string()));
        }
        self.map.insert(key, id.0);
        Ok(())
    }

    /// Resolve a FANOUT / FANIN identifier list, defining each new name.
    ///
    /// Numbers and reserved words look up a stream. A new name is bound to
    /// the lowest stream number not already taken by this list or by a name
    /// already on the stage. The same name twice in the list is a duplicate.
    pub fn resolve_list<S: AsRef<str>>(
        &mut self,
        tokens: &[S],
    ) -> Result<Vec<StreamId>, StreamIdentError> {
        let mut out = Vec::with_capacity(tokens.len());
        let mut seen: HashSet<String> = HashSet::new();
        let mut taken: HashSet<u32> = self.map.values().copied().collect();
        for token in tokens {
            let t = token.as_ref().trim();
            if t.is_empty() {
                return Err(StreamIdentError::Unknown(token.as_ref().to_string()));
            }
            if let Ok(n) = t.parse::<u32>() {
                taken.insert(n);
                out.push(StreamId(n));
                continue;
            }
            let key = t.to_ascii_uppercase();
            if !seen.insert(key.clone()) {
                return Err(StreamIdentError::Duplicate(t.to_string()));
            }
            if let Some(id) = reserved_stream(&key) {
                taken.insert(id);
                out.push(StreamId(id));
                continue;
            }
            if let Some(&id) = self.map.get(&key) {
                taken.insert(id);
                out.push(StreamId(id));
                continue;
            }
            let id = lowest_unused(&taken);
            taken.insert(id);
            self.map.insert(key, id);
            out.push(StreamId(id));
        }
        Ok(out)
    }

    pub fn get(&self, name: &str) -> Option<StreamId> {
        self.map
            .get(&name.trim().to_ascii_uppercase())
            .copied()
            .map(StreamId)
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

/// `PRIMARY` / `SECONDARY` / `TERTIARY` (already uppercased).
pub fn reserved_stream(upper: &str) -> Option<u32> {
    match upper {
        "PRIMARY" => Some(0),
        "SECONDARY" => Some(1),
        "TERTIARY" => Some(2),
        _ => None,
    }
}

fn lowest_unused(taken: &HashSet<u32>) -> u32 {
    let mut n = 0u32;
    while taken.contains(&n) {
        n += 1;
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_and_reserved_words() {
        let names = StreamNames::new();
        assert_eq!(names.resolve("0").unwrap(), StreamId::PRIMARY);
        assert_eq!(names.resolve("2").unwrap(), StreamId::TERTIARY);
        assert_eq!(names.resolve("PRIMARY").unwrap(), StreamId::PRIMARY);
        assert_eq!(names.resolve("secondary").unwrap(), StreamId::SECONDARY);
        assert_eq!(names.resolve("Tertiary").unwrap(), StreamId::TERTIARY);
        assert_eq!(names.resolve("99").unwrap(), StreamId(99));
    }

    #[test]
    fn unknown_name_is_an_error() {
        let names = StreamNames::new();
        assert_eq!(
            names.resolve("FOO"),
            Err(StreamIdentError::Unknown("FOO".into()))
        );
        assert_eq!(
            names.resolve("foo").unwrap_err().to_string(),
            "unknown stream identifier foo"
        );
    }

    #[test]
    fn define_then_resolve_is_case_insensitive() {
        let mut names = StreamNames::new();
        names.define("Even", StreamId(3)).unwrap();
        assert_eq!(names.resolve("even").unwrap(), StreamId(3));
        assert_eq!(names.resolve("EVEN").unwrap(), StreamId(3));
        assert_eq!(
            names.define("EVEN", StreamId(4)),
            Err(StreamIdentError::Duplicate("EVEN".into()))
        );
        assert_eq!(
            names.define("PRIMARY", StreamId(9)),
            Err(StreamIdentError::Reserved("PRIMARY".into()))
        );
        assert_eq!(
            names
                .define("primary", StreamId(9))
                .unwrap_err()
                .to_string(),
            "cannot rename reserved stream identifier primary"
        );
        assert_eq!(
            names.define("EVEN", StreamId(3)).unwrap_err().to_string(),
            "duplicate stream identifier EVEN"
        );
    }

    #[test]
    fn list_first_use_binds_lowest_unused() {
        let mut names = StreamNames::new();
        assert_eq!(
            names.resolve_list(&["LEFT", "RIGHT"]).unwrap(),
            vec![StreamId(0), StreamId(1)]
        );
        assert_eq!(names.get("left"), Some(StreamId(0)));
        assert_eq!(names.get("right"), Some(StreamId(1)));
        assert_eq!(
            names.resolve_list(&["1", "LEFT"]).unwrap(),
            vec![StreamId(1), StreamId(0)],
            "LEFT already bound"
        );
    }

    #[test]
    fn list_new_name_after_a_number_takes_lowest_free() {
        let mut names = StreamNames::new();
        assert_eq!(
            names.resolve_list(&["1", "SPARE"]).unwrap(),
            vec![StreamId(1), StreamId(0)]
        );
        assert_eq!(names.get("spare"), Some(StreamId(0)));
    }

    #[test]
    fn list_duplicate_name_is_an_error() {
        let mut names = StreamNames::new();
        assert_eq!(
            names.resolve_list(&["FOO", "FOO"]),
            Err(StreamIdentError::Duplicate("FOO".into()))
        );
        assert_eq!(
            names
                .resolve_list(&["PRIMARY", "primary"])
                .unwrap_err()
                .to_string(),
            "duplicate stream identifier primary"
        );
    }

    #[test]
    fn list_reserved_words_do_not_fill_the_map() {
        let mut names = StreamNames::new();
        assert_eq!(
            names
                .resolve_list(&["PRIMARY", "SECONDARY", "TERTIARY"])
                .unwrap(),
            vec![StreamId(0), StreamId(1), StreamId(2)]
        );
        assert!(names.is_empty());
    }
}
