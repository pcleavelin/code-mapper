//! Call resolution: every symbol's callees and callers, from a server's (file, line) targets or
//! from tree-sitter call sites resolved by name.

use super::{Call, File, Index, Qual, SymRef};
use std::collections::HashMap;

impl Index {
    fn owner_is(&self, r: SymRef, t: &str) -> bool {
        self.sym(r).owner.as_deref() == Some(t)
    }

    fn in_module(&self, r: SymRef, m: &str) -> bool {
        let f = &self.files[r.file];
        f.stem() == m || f.dir() == m
    }

    /// Where a call goes. Qualifier-aware, no types: `self.m` / `Self::m` bind to the caller's
    /// owner; `T::m` / `t.m` to a member of type `T`, else to module (file stem or directory)
    /// `T`, else through the file's imports; a lowercase receiver picks a member somewhere;
    /// an unqualified name picks a free function in the same file, then an imported one, then
    /// one in the same directory, then anywhere. Wrong when two types share a method name and
    /// the receiver is a variable; a real resolver needs per-language scope and type rules.
    fn resolve(
        &self,
        from: SymRef,
        call: &Call,
        by_name: &HashMap<String, Vec<SymRef>>,
    ) -> Option<SymRef> {
        // a struct/enum/union is data, not a call target, even where `Foo{...}` parses as a call
        let cands: Vec<SymRef> = by_name
            .get(&call.name)?
            .iter()
            .copied()
            .filter(|r| {
                !["struct", "enum", "union"]
                    .iter()
                    .any(|k| self.sym(*r).kind.contains(k))
            })
            .collect();
        let f = &self.files[from.file];
        let owner = self.sym(from).owner.as_deref();
        let same_file = |r: &&SymRef| r.file == from.file;
        let member = |r: &&SymRef| self.sym(**r).owner.is_some();
        let free = |r: &&SymRef| self.sym(**r).owner.is_none();
        let pick = |it: &mut dyn Iterator<Item = &SymRef>| -> Option<SymRef> {
            let v: Vec<SymRef> = it.copied().collect();
            v.iter()
                .find(|r| r.file == from.file)
                .or(v.first())
                .copied()
        };

        match &call.qual {
            Qual::SelfRef => owner
                .and_then(|o| pick(&mut cands.iter().filter(|r| self.owner_is(**r, o))))
                .or_else(|| pick(&mut cands.iter().filter(same_file)))
                .or_else(|| pick(&mut cands.iter())),
            Qual::Some(q) => pick(&mut cands.iter().filter(|r| self.owner_is(**r, q)))
                .or_else(|| pick(&mut cands.iter().filter(|r| self.in_module(**r, q))))
                .or_else(|| {
                    if let Some(m) = f.imports.get(q) {
                        // a known import that matched no file here is an external module (`log.error`)
                        return pick(&mut cands.iter().filter(|r| self.in_module(**r, m)));
                    }
                    let lowercase = q
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_lowercase() || c == '_');
                    if !lowercase {
                        return None; // `Regex::new`: a type this repo does not define
                    }
                    // a variable receiver: some type's member, the caller's own type first; a
                    // variable never calls a free function
                    owner
                        .and_then(|o| pick(&mut cands.iter().filter(|r| self.owner_is(**r, o))))
                        .or_else(|| pick(&mut cands.iter().filter(member)))
                }),
            Qual::None => pick(&mut cands.iter().filter(same_file).filter(free))
                .or_else(|| {
                    f.imports
                        .get(&call.name)
                        .and_then(|m| pick(&mut cands.iter().filter(|r| self.in_module(**r, m))))
                })
                .or_else(|| {
                    pick(
                        &mut cands
                            .iter()
                            .filter(free)
                            .filter(|r| self.files[r.file].dir() == f.dir()),
                    )
                })
                .or_else(|| {
                    // only C reaches other files without naming them (headers); elsewhere an
                    // unqualified name that is not local or imported is a builtin or a std call
                    let c = f.path.ends_with(".c") || f.path.ends_with(".h");
                    if c {
                        pick(&mut cands.iter().filter(free)).or_else(|| pick(&mut cands.iter()))
                    } else {
                        None
                    }
                }),
        }
    }

    /// Rebuilds every callee / caller list: server targets by (file, line), tree-sitter calls
    /// by name.
    pub fn link(&mut self) {
        let mut by_name: HashMap<String, Vec<SymRef>> = HashMap::new();
        for (file, f) in self.files.iter().enumerate() {
            for (sym, s) in f.symbols.iter().enumerate() {
                by_name
                    .entry(s.name.clone())
                    .or_default()
                    .push(SymRef { file, sym });
            }
        }

        // a server names its targets by path and line, once per call site, so the file lookup
        // is a map rather than `find_file`'s scan over every file
        let file_of: HashMap<&str, usize> = self
            .files
            .iter()
            .enumerate()
            .map(|(i, f)| (f.path.as_str(), i))
            .collect();

        let mut edges = Vec::new();
        for (file, f) in self.files.iter().enumerate() {
            for (sym, s) in f.symbols.iter().enumerate() {
                let from = SymRef { file, sym };
                for call in &s.calls {
                    if let Some(to) = self.resolve(from, call, &by_name) {
                        edges.push((from, to));
                    }
                }
                for (path, line) in &s.targets {
                    let Some(&tf) = file_of.get(path.as_str()) else {
                        continue;
                    };
                    if let Some(to) = self.sym_at(tf, *line as usize) {
                        edges.push((from, to));
                    }
                }
            }
        }
        edges.sort_by_key(|(a, b)| (a.file, a.sym, b.file, b.sym));
        edges.dedup();

        for f in &mut self.files {
            for s in &mut f.symbols {
                s.callees.clear();
                s.callers.clear();
            }
        }
        for (from, to) in edges {
            self.files[from.file].symbols[from.sym].callees.push(to);
            self.files[to.file].symbols[to.sym].callers.push(from);
        }
    }

    /// A copy with the symbol tables and imports but no text, which is all `link` reads:
    /// small enough to hand to a thread. `take_edges` brings the result back.
    pub fn symbols_only(&self) -> Index {
        let files = self
            .files
            .iter()
            .map(|f| File {
                path: f.path.clone(),
                lines: Vec::new(),
                hl: Vec::new(),
                symbols: f.symbols.clone(),
                imports: f.imports.clone(),
                mtime: f.mtime,
                hash: f.hash,
                backend: f.backend,
                pending: f.pending,
            })
            .collect();
        Index {
            root: self.root.clone(),
            files,
        }
    }

    /// Copy every symbol's callees and callers from `linked`, a `symbols_only` copy of this
    /// same index after `link`. Symbol tables must match; a mismatch means the index changed
    /// under the thread and the copy is stale.
    pub fn take_edges(&mut self, linked: &Index) {
        let same = self.files.len() == linked.files.len()
            && self.files.iter().zip(&linked.files).all(|(a, b)| {
                a.hash == b.hash
                    && a.backend == b.backend
                    && a.pending == b.pending
                    && a.symbols.len() == b.symbols.len()
            });
        if !same {
            return;
        }
        for (f, l) in self.files.iter_mut().zip(&linked.files) {
            for (s, t) in f.symbols.iter_mut().zip(&l.symbols) {
                s.callees.clone_from(&t.callees);
                s.callers.clone_from(&t.callers);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Parsers, parse_file};
    use super::*;

    fn index(files: &[(&str, &str)]) -> Index {
        let mut p = Parsers::default();
        let files = files
            .iter()
            .map(|(path, src)| {
                parse_file(
                    &mut p,
                    path.to_string(),
                    src,
                    path.rsplit('.').next().unwrap(),
                )
            })
            .collect();
        let mut idx = Index {
            root: ".".into(),
            files,
        };
        idx.link();
        idx
    }

    fn callee_names(idx: &Index, name: &str, file: &str) -> Vec<String> {
        let fi = idx.find_file(file).unwrap();
        let si = idx.files[fi]
            .symbols
            .iter()
            .position(|s| s.name == name)
            .unwrap();
        idx.sym(SymRef { file: fi, sym: si })
            .callees
            .iter()
            .map(|r| format!("{}:{}", idx.files[r.file].path, idx.sym(*r).name))
            .collect()
    }

    #[test]
    fn links_calls_by_name() {
        let src = "fn a() { b(); c::d(); }\nfn b() {}\nstruct C;\nimpl C { fn d() { b() } }\n";
        let idx = index(&[("x.rs", src)]);
        let names: Vec<&str> = idx.files[0]
            .symbols
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        assert_eq!(names, ["a", "b", "C", "impl C", "d"]);
        assert_eq!(idx.files[0].symbols[4].owner.as_deref(), Some("C"));

        let a = idx.find_symbols("a")[0];
        let callees: Vec<&str> = idx
            .sym(a)
            .callees
            .iter()
            .map(|r| idx.sym(*r).name.as_str())
            .collect();
        assert_eq!(callees, ["b", "d"]);
        assert_eq!(idx.sym(idx.find_symbols("b")[0]).callers.len(), 2);
        assert_eq!(idx.roots(), [a]);
        assert_eq!(idx.call_tree(a, 5).len(), 3);
    }

    #[test]
    fn resolves_by_owner_module_and_import() {
        let a = "pub struct A;\nimpl A { pub fn new() -> A { A } fn go(&self) { self.step(); Self::new(); } fn step(&self) {} }\npub fn helper() {}\n";
        let b = "use crate::a::helper;\npub struct B;\nimpl B { pub fn new() -> B { B } fn step(&self) {} }\nfn run(x: &B) { A::new(); B::new(); x.step(); helper(); a::helper(); }\nfn helper() {}\n";
        let idx = index(&[("src/a.rs", a), ("src/b.rs", b)]);

        // self.step / Self::new bind to A's members, not B's
        assert_eq!(
            callee_names(&idx, "go", "src/a.rs"),
            ["src/a.rs:new", "src/a.rs:step"]
        );
        // A::new -> A's new, B::new -> B's new, x.step -> a member (same file wins), helper -> same-file free fn,
        // a::helper -> module a
        let run = callee_names(&idx, "run", "src/b.rs");
        assert!(run.contains(&"src/a.rs:new".to_string()), "{run:?}");
        assert!(run.contains(&"src/b.rs:new".to_string()), "{run:?}");
        assert!(run.contains(&"src/b.rs:step".to_string()), "{run:?}");
        assert!(run.contains(&"src/b.rs:helper".to_string()), "{run:?}");
        assert!(run.contains(&"src/a.rs:helper".to_string()), "{run:?}");
        assert_eq!(
            idx.files[1].imports.get("helper").map(String::as_str),
            Some("a")
        );
    }

    #[test]
    fn odin_package_calls() {
        let main = "package main\nimport \"core\"\nimport \"util\"\nimport \"core:log\"\nS :: struct { commands: int }\nmain :: proc() {\n    core.init_bookmarks(nil)\n    util.make_static_list(int, 4)\n    helper()\n    make([]int, 4)\n    s := S{ commands = make(int) }\n    log.error(\"x\")\n}\nhelper :: proc() {}\n";
        let core =
            "package core\ninit_bookmarks :: proc(b: rawptr) {}\nerror :: proc(msg: string) {}\n";
        let util =
            "package util\nmake_static_list :: proc($T: typeid, n: int) {}\nmake :: proc() {}\n";
        let idx = index(&[
            ("src/main.odin", main),
            ("src/core/bookmarks.odin", core),
            ("src/util/list.odin", util),
        ]);
        let m = idx.find_symbols("main")[0];
        let calls: Vec<String> = idx
            .sym(m)
            .calls
            .iter()
            .map(|c| format!("{}/{:?}", c.name, c.qual))
            .collect();
        assert!(
            calls.iter().any(|c| c == "init_bookmarks/Some(\"core\")"),
            "{calls:?}"
        );
        let mut got = callee_names(&idx, "main", "src/main.odin");
        got.sort();
        assert_eq!(
            got,
            [
                "src/core/bookmarks.odin:init_bookmarks",
                "src/main.odin:helper",
                "src/util/list.odin:make_static_list"
            ],
            "{got:?}"
        );
    }
}
