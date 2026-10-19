use pipe_core::StageFn;

use crate::builtins;

type Factory = Box<dyn Fn(Vec<String>) -> StageFn>;

struct Entry {
    /// Canonical IBM verb, stored uppercased.
    name: String,
    /// Minimum abbreviation length from the 1.1.11 diagram (full length = exact only).
    min_len: usize,
    factory: Factory,
}

/// Looks up built-in (and optionally user) stage verbs.
pub struct StageRegistry {
    entries: Vec<Entry>,
}

impl StageRegistry {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Register a verb that must be spelled in full (no diagram abbreviation).
    pub fn register<F>(&mut self, name: &str, f: F)
    where
        F: Fn(Vec<String>) -> StageFn + 'static,
    {
        let name = name.to_ascii_uppercase();
        self.register_abbrev(&name, name.len(), f);
    }

    /// Register a verb whose 1.1.11 syntax diagram shows a lowercase suffix.
    ///
    /// `min_len` is the length of the uppercase prefix (`UNIQue` → 4, `DUPlicate` → 3,
    /// `TRUNCate` → 5, `PREDSELect` → 7). Tokens shorter than that, or that are not
    /// a prefix of `name`, are not this verb.
    pub fn register_abbrev<F>(&mut self, name: &str, min_len: usize, f: F)
    where
        F: Fn(Vec<String>) -> StageFn + 'static,
    {
        let name = name.to_ascii_uppercase();
        debug_assert!(
            min_len >= 1 && min_len <= name.len(),
            "abbreviation length {min_len} is not valid for {name}"
        );
        self.entries.push(Entry {
            name,
            min_len,
            factory: Box::new(f),
        });
    }

    pub fn create(&self, verb: &str, args: Vec<String>) -> Option<StageFn> {
        self.find(verb).map(|e| (e.factory)(args))
    }

    pub fn contains(&self, verb: &str) -> bool {
        self.find(verb).is_some()
    }

    fn find(&self, verb: &str) -> Option<&Entry> {
        let key = verb.to_ascii_uppercase();
        if let Some(e) = self.entries.iter().find(|e| e.name == key) {
            return Some(e);
        }
        let mut found = None;
        for e in &self.entries {
            if matches_verb_abbrev(&key, &e.name, e.min_len) {
                if found.is_some() {
                    return None;
                }
                found = Some(e);
            }
        }
        found
    }
}

/// IBM verb abbreviation: `key` matches `name` when `min_len ≤ len(key) ≤ len(name)`.
fn matches_verb_abbrev(key: &str, name: &str, min_len: usize) -> bool {
    key.len() >= min_len && key.len() <= name.len() && name.starts_with(key)
}

impl Default for StageRegistry {
    fn default() -> Self {
        let mut r = Self::new();
        register_builtins(&mut r);
        r
    }
}

pub fn register_builtins(r: &mut StageRegistry) {
    // IBM Appendix A verbs only (plus documented synonyms). Mixed-case diagram
    // prefixes (1.1.11 Ch. 19) use register_abbrev; all-caps verbs are exact.
    r.register("LITERAL", |a| builtins::literal(a));
    r.register_abbrev("CONSOLE", 4, |a| builtins::console(a)); // CONSole p.303
    r.register_abbrev("TERMINAL", 4, |a| builtins::console(a)); // TERMinal p.303 synonym
                                                                // Portable line-oriented stand-in for CMS `disk` / `diskfast` family.
    r.register("DISK", |a| builtins::disk(a));
    // IBM file stages (1.1.11 pp.230, 237, 245); operand is a host path.
    r.register("<", |a| builtins::file_in(a));
    r.register(">", |a| builtins::file_replace(a));
    r.register(">>", |a| builtins::file_append(a));
    // Portable process runner; IBM COMMAND issues CMS/TSO commands.
    r.register("COMMAND", |a| builtins::command(a));
    r.register("CMD", |a| builtins::command(a)); // 1.1.11 p.300 synonym
    r.register("LOCATE", |a| builtins::locate(a));
    r.register("NLOCATE", |a| builtins::nlocate(a));
    r.register("NOTLOCATE", |a| builtins::nlocate(a)); // documented synonym
    r.register("TAKE", |a| builtins::take(a));
    r.register("DROP", |a| builtins::drop(a));
    r.register("STRIP", |a| builtins::strip(a));
    r.register("CHANGE", |a| builtins::change(a));
    r.register("COUNT", |a| builtins::count(a));
    r.register("SORT", |a| builtins::sort(a));
    r.register("BUFFER", |a| builtins::buffer(a));
    r.register("FANOUT", |a| builtins::fanout(a));
    r.register("FANIN", |a| builtins::fanin(a));
    r.register("FANINANY", |a| builtins::faninany(a));
    r.register("HOLE", |a| builtins::hole(a));
    r.register("SPEC", |a| builtins::spec(a));
    r.register_abbrev("SPECS", 4, |a| builtins::spec(a)); // SPECs p.496 keyword → SPEC
    r.register("COPY", |a| builtins::copy(a));
    r.register("BETWEEN", |a| builtins::between(a));
    r.register("SPLIT", |a| builtins::split(a));
    r.register("JOIN", |a| builtins::join(a));
    r.register("JOINCONT", |a| builtins::joincont(a));
    r.register("CHOP", |a| builtins::chop(a));
    r.register_abbrev("TRUNCATE", 5, |a| builtins::chop(a)); // TRUNCate p.289 synonym
    r.register_abbrev("UNIQUE", 4, |a| builtins::unique(a)); // UNIQue p.572
    r.register_abbrev("DUPLICATE", 3, |a| builtins::duplicate(a)); // DUPlicate p.334
    r.register("PICK", |a| builtins::pick(a)); // PICK p.454 (diagram is all caps)
    r.register("ZONE", |a| builtins::zone(a)); // ZONE p.610 (diagram is all caps)
    r.register("PREFACE", |a| builtins::preface(a)); // PREFACE p.461 (diagram is all caps)
    r.register("APPEND", |a| builtins::append(a)); // APPEND p.263 (diagram is all caps)
    r.register("LOOKUP", |a| builtins::lookup(a)); // LOOKUP p.416 (diagram is all caps)
    r.register("ELASTIC", |a| builtins::elastic(a)); // ELASTIC p.335 (diagram is all caps)
    r.register("GATE", |a| builtins::gate(a)); // GATE p.365 (diagram is all caps)
    r.register("IF", |a| builtins::if_stage(a)); // IF p.389 (diagram is all caps)
    r.register("DAM", |a| builtins::dam(a)); // DAM p.310 (diagram is all caps)
    r.register("JUXTAPOSE", |a| builtins::juxtapose(a)); // JUXTAPOSE p.403 (diagram is all caps)
                                                         // SYNChronise / SYNChronize p.536. Shared SYNC…SYNCHRONI prefixes resolve
                                                         // to the British spelling so the two synonym entries are not ambiguous.
    r.register_abbrev("SYNCHRONISE", 4, |a| builtins::synchronise(a));
    r.register_abbrev("SYNCHRONIZE", 10, |a| builtins::synchronise(a));
    r.register_abbrev("PREDSELECT", 7, |a| builtins::predselect(a)); // PREDSELect p.459
    r.register("NOEOFBACK", |a| builtins::noeofback(a)); // NOEOFBACK p.440 (diagram is all caps)
    r.register("NOTEOFBACK", |a| builtins::noeofback(a)); // p.441 synonym
    r.register("EOFBACK", |a| builtins::eofback(a)); // EOFBACK p.337 (diagram is all caps)
    r.register("AGGRC", |a| builtins::aggrc(a)); // AGGRC p.258 (diagram is all caps)
    r.register("DELAY", |a| builtins::delay(a)); // DELAY p.321 (diagram is all caps)
    r.register("BEAT", |a| builtins::beat(a)); // BEAT p.271 (diagram is all caps)
    r.register("UPDATE", |a| builtins::update(a)); // UPDATE p.576 (diagram is all caps)
    r.register("PIPCMD", |a| builtins::pipcmd(a)); // PIPCMD p.457 (diagram is all caps)
    r.register("PIPESTOP", |a| builtins::pipestop(a)); // PIPESTOP p.458 (diagram is all caps)
}
