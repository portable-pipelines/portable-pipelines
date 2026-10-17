use std::collections::HashMap;

use pipe_core::StageFn;

use crate::builtins;

type Factory = Box<dyn Fn(Vec<String>) -> StageFn>;

/// Looks up built-in (and optionally user) stage verbs.
pub struct StageRegistry {
    factories: HashMap<String, Factory>,
}

impl StageRegistry {
    pub fn new() -> Self {
        Self {
            factories: HashMap::new(),
        }
    }

    pub fn register<F>(&mut self, name: &str, f: F)
    where
        F: Fn(Vec<String>) -> StageFn + 'static,
    {
        self.factories
            .insert(name.to_ascii_uppercase(), Box::new(f));
    }

    pub fn create(&self, verb: &str, args: Vec<String>) -> Option<StageFn> {
        self.factories
            .get(&verb.to_ascii_uppercase())
            .map(|f| f(args))
    }

    pub fn contains(&self, verb: &str) -> bool {
        self.factories.contains_key(&verb.to_ascii_uppercase())
    }
}

impl Default for StageRegistry {
    fn default() -> Self {
        let mut r = Self::new();
        register_builtins(&mut r);
        r
    }
}

pub fn register_builtins(r: &mut StageRegistry) {
    // IBM Appendix A verbs only (plus SPECS → SPEC keyword synonym).
    r.register("LITERAL", |a| builtins::literal(a));
    r.register("CONSOLE", |a| builtins::console(a));
    r.register("TERMINAL", |a| builtins::console(a)); // 1.1.11 p.305 synonym
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
    r.register("SPECS", |a| builtins::spec(a)); // Appendix A keyword → SPEC
    r.register("COPY", |a| builtins::copy(a));
    r.register("BETWEEN", |a| builtins::between(a));
    r.register("SPLIT", |a| builtins::split(a));
    r.register("JOIN", |a| builtins::join(a));
    r.register("JOINCONT", |a| builtins::joincont(a));
    r.register("CHOP", |a| builtins::chop(a));
    // 1.1.11 p.289 synonym TRUNCate (min 5).
    r.register("TRUNCATE", |a| builtins::chop(a));
    r.register("TRUNC", |a| builtins::chop(a));
    r.register("TRUNCA", |a| builtins::chop(a));
    r.register("TRUNCAT", |a| builtins::chop(a));
    r.register("UNIQUE", |a| builtins::unique(a));
}
