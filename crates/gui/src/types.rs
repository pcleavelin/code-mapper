use std::collections::{BTreeMap, BTreeSet};

use domain::{Index, Line, SourceFile, Span, Symbol, SymbolId};

use crate::sequence::Unit;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum TypeKind {
    Struct,
    Enum,
    Trait,
    Alias,
}

impl TypeKind {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Struct => "struct",
            Self::Enum => "enum",
            Self::Trait => "trait",
            Self::Alias => "type",
        }
    }

    fn of(kind: &str) -> Option<Self> {
        if kind.contains("struct") {
            Some(Self::Struct)
        } else if kind.contains("enum") {
            Some(Self::Enum)
        } else if kind.contains("interface") || kind.contains("trait") {
            Some(Self::Trait)
        } else if kind == "type" || kind == "type_item" {
            Some(Self::Alias)
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Access {
    Construct,
    Read,
    Mutate,
    Consume,
    Return,
    Lend,
}

impl Access {
    pub(crate) const ALL: [Self; 6] = [
        Self::Construct,
        Self::Read,
        Self::Mutate,
        Self::Consume,
        Self::Return,
        Self::Lend,
    ];

    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Construct => "new",
            Self::Read => "read",
            Self::Mutate => "mut",
            Self::Consume => "take",
            Self::Return => "ret",
            Self::Lend => "ref",
        }
    }
}

pub(crate) type TypeIx = usize;
pub(crate) type FnIx = usize;

#[derive(Clone, Debug)]
pub(crate) struct TypeNode {
    pub(crate) symbol: SymbolId,
    pub(crate) name: String,
    pub(crate) krate: String,
    pub(crate) file: String,
    pub(crate) line: u32,
    pub(crate) kind: TypeKind,
    pub(crate) copy: bool,
    pub(crate) holds: BTreeSet<TypeIx>,
    pub(crate) traits: BTreeSet<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct FnNode {
    pub(crate) symbol: SymbolId,
    pub(crate) name: String,
    pub(crate) owner: Option<TypeIx>,
    pub(crate) owner_name: Option<String>,
    pub(crate) krate: String,
    pub(crate) span: Span,
    pub(crate) uses: BTreeMap<TypeIx, BTreeSet<Access>>,
}

impl FnNode {
    pub(crate) fn label(&self) -> String {
        match &self.owner_name {
            Some(owner) => format!("{owner}::{}", self.name),
            None => self.name.clone(),
        }
    }

    pub(crate) fn inputs(&self) -> impl Iterator<Item = TypeIx> + '_ {
        self.uses.iter().filter_map(|(ty, accesses)| {
            accesses
                .iter()
                .any(|access| matches!(access, Access::Read | Access::Mutate | Access::Consume))
                .then_some(*ty)
        })
    }

    pub(crate) fn outputs(&self) -> impl Iterator<Item = TypeIx> + '_ {
        self.uses.iter().filter_map(|(ty, accesses)| {
            accesses
                .iter()
                .any(|access| matches!(access, Access::Return | Access::Construct))
                .then_some(*ty)
        })
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct TypeModel {
    pub(crate) types: Vec<TypeNode>,
    pub(crate) fns: Vec<FnNode>,
    pub(crate) type_of: BTreeMap<SymbolId, TypeIx>,
    pub(crate) fn_of: BTreeMap<SymbolId, FnIx>,
    pub(crate) touching: BTreeMap<TypeIx, Vec<FnIx>>,
    pub(crate) held_by: BTreeMap<TypeIx, BTreeSet<TypeIx>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Tok {
    Ident(String),
    Punct(char),
}

fn tokens(text: &str) -> Vec<Tok> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.split("//").next().unwrap_or_default();
        let mut chars = line.chars().peekable();
        while let Some(character) = chars.next() {
            if character == '"' {
                let mut escaped = false;
                for inner in chars.by_ref() {
                    if escaped {
                        escaped = false;
                    } else if inner == '\\' {
                        escaped = true;
                    } else if inner == '"' {
                        break;
                    }
                }
            } else if character == '\'' {
                while chars
                    .peek()
                    .is_some_and(|next| next.is_alphanumeric() || *next == '_')
                {
                    chars.next();
                }
                if chars.peek() == Some(&'\'') {
                    chars.next();
                }
            } else if character.is_alphabetic() || character == '_' {
                let mut word = String::from(character);
                while let Some(next) = chars.peek() {
                    if next.is_alphanumeric() || *next == '_' {
                        word.push(*next);
                        chars.next();
                    } else {
                        break;
                    }
                }
                out.push(Tok::Ident(word));
            } else if !character.is_whitespace() {
                out.push(Tok::Punct(character));
            }
        }
    }
    out
}

fn symbol_text(file: &SourceFile, span: Span) -> String {
    let mut text = String::new();
    for number in span.start().value()..=span.end().value() {
        if let Some(line) = file.text().line(Line::new(number)) {
            text.push_str(line.as_str());
            text.push('\n');
        }
    }
    text
}

fn derives_copy(file: &SourceFile, span: Span) -> bool {
    let start = span.start().value();
    (start.saturating_sub(4)..=start).any(|number| {
        file.text().line(Line::new(number)).is_some_and(|line| {
            let text = line.as_str();
            text.contains("derive") && text.contains("Copy")
        })
    })
}

fn is_type_word(word: &str) -> bool {
    word.chars().next().is_some_and(char::is_uppercase)
}

fn matching(toks: &[Tok], open: usize, left: char, right: char) -> usize {
    let mut depth = 0usize;
    for (at, tok) in toks.iter().enumerate().skip(open) {
        match tok {
            Tok::Punct(character) if *character == left => depth += 1,
            Tok::Punct(character) if *character == right => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return at;
                }
            }
            _ => {}
        }
    }
    toks.len()
}

struct Resolver<'model> {
    imports: &'model BTreeMap<String, BTreeMap<String, String>>,
    by_name: &'model BTreeMap<String, Vec<TypeIx>>,
    types: &'model [TypeNode],
}

impl Resolver<'_> {
    fn resolve(&self, name: &str, file: &SourceFile) -> Option<TypeIx> {
        let candidates = self.by_name.get(name)?;
        let path = file.path().as_str();
        let here = Unit::Crate.of(path);
        let import = self
            .imports
            .get(path)
            .and_then(|table| table.get(name))
            .map(String::as_str);
        let imported = |node: &TypeNode| {
            import.is_some_and(|root| {
                if root == "crate" || root == "self" || root == "super" {
                    node.krate == here
                } else {
                    node.krate == root.replace('_', "-")
                }
            })
        };
        let pick = |test: &dyn Fn(&TypeNode) -> bool| {
            candidates
                .iter()
                .copied()
                .find(|ix| self.types.get(*ix).is_some_and(test))
        };
        pick(&|node| node.file == path)
            .or_else(|| pick(&|node| imported(node)))
            .or_else(|| {
                if import.is_some_and(|root| !matches!(root, "crate" | "self" | "super")) {
                    None
                } else {
                    pick(&|node| node.krate == here)
                }
            })
    }
}

fn use_table(file: &SourceFile) -> BTreeMap<String, String> {
    let mut table = BTreeMap::new();
    let count = file.text().count().value();
    let mut statement = String::new();
    let mut open = false;
    for number in 0..count {
        let Some(line) = file.text().line(Line::new(number)) else {
            continue;
        };
        let text = line.as_str().trim();
        if !open {
            let starts = text.starts_with("use ")
                || text.starts_with("pub use ")
                || text.starts_with("pub(crate) use ");
            if !starts {
                continue;
            }
            open = true;
            statement.clear();
        }
        statement.push_str(text);
        statement.push(' ');
        if text.contains(';') {
            open = false;
            let toks = tokens(&statement);
            let mut words = toks.iter().filter_map(|tok| match tok {
                Tok::Ident(word) if word != "pub" && word != "use" => Some(word.clone()),
                _ => None,
            });
            let Some(root) = words.next() else {
                continue;
            };
            let mut previous_as = false;
            for tok in &toks {
                if let Tok::Ident(word) = tok {
                    if word == "as" {
                        previous_as = true;
                        continue;
                    }
                    if is_type_word(word) || previous_as {
                        table.insert(word.clone(), root.clone());
                    }
                    previous_as = false;
                }
            }
        }
    }
    table
}

struct Impl {
    span: Span,
    ty: String,
    trait_name: Option<String>,
}

fn parse_impl(name: &str) -> Option<Impl> {
    let toks = tokens(name);
    let mut at = 0;
    if toks.first() == Some(&Tok::Ident("impl".to_owned())) {
        at = 1;
    }
    if toks.get(at) == Some(&Tok::Punct('<')) {
        at = matching(&toks, at, '<', '>') + 1;
    }
    let rest = toks.get(at..).unwrap_or_default();
    let split = rest
        .iter()
        .position(|tok| *tok == Tok::Ident("for".to_owned()));
    let head_ident = |part: &[Tok]| {
        let mut last = None;
        for tok in part {
            match tok {
                Tok::Ident(word) if word != "dyn" && word != "mut" => last = Some(word.clone()),
                Tok::Punct('<') => break,
                _ => {}
            }
        }
        last
    };
    match split {
        Some(at_for) => Some(Impl {
            span: Span::new(Line::new(0), Line::new(0))?,
            ty: head_ident(rest.get(at_for + 1..).unwrap_or_default())?,
            trait_name: head_ident(rest.get(..at_for).unwrap_or_default()),
        }),
        None => Some(Impl {
            span: Span::new(Line::new(0), Line::new(0))?,
            ty: head_ident(rest)?,
            trait_name: None,
        }),
    }
}

fn kind_of(symbol: &Symbol) -> String {
    symbol.kind().as_str().to_owned()
}

fn is_fn(kind: &str) -> bool {
    kind.contains("function") || kind == "method"
}

fn is_rust(file: &SourceFile) -> bool {
    file.path().as_str().ends_with(".rs")
}

fn split_top(toks: &[Tok]) -> Vec<&[Tok]> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (at, tok) in toks.iter().enumerate() {
        match tok {
            Tok::Punct('(' | '<' | '[' | '{') => depth += 1,
            Tok::Punct(')' | '>' | ']' | '}') => depth -= 1,
            Tok::Punct(',') if depth == 0 => {
                parts.push(toks.get(start..at).unwrap_or_default());
                start = at + 1;
            }
            _ => {}
        }
    }
    if start < toks.len() {
        parts.push(toks.get(start..).unwrap_or_default());
    }
    parts
}

fn access_of(type_toks: &[Tok]) -> Access {
    let mut reference = false;
    for pair in type_toks.windows(2) {
        if pair.first() == Some(&Tok::Punct('&')) {
            reference = true;
            if pair.get(1) == Some(&Tok::Ident("mut".to_owned())) {
                return Access::Mutate;
            }
        }
    }
    if reference || type_toks.first() == Some(&Tok::Punct('&')) {
        Access::Read
    } else {
        Access::Consume
    }
}

pub(crate) fn type_model(index: &Index) -> TypeModel {
    let mut model = TypeModel::default();
    let mut ids_by_file: BTreeMap<domain::FileId, Vec<SymbolId>> = BTreeMap::new();
    for id in index.symbol_ids() {
        ids_by_file.entry(id.file()).or_default().push(id);
    }
    let symbols_of = |file: domain::FileId| {
        ids_by_file
            .get(&file)
            .into_iter()
            .flatten()
            .filter_map(|id| index.symbol(*id).map(|symbol| (*id, symbol)))
    };
    for entry in index.file_entries() {
        let file = entry.item;
        if !is_rust(file) {
            continue;
        }
        let path = file.path().as_str().to_owned();
        let krate = Unit::Crate.of(&path);
        for (id, symbol) in symbols_of(entry.id) {
            let Some(kind) = TypeKind::of(&kind_of(symbol)) else {
                continue;
            };
            model.type_of.insert(id, model.types.len());
            model.types.push(TypeNode {
                symbol: id,
                name: symbol.name().as_str().to_owned(),
                krate: krate.clone(),
                file: path.clone(),
                line: symbol.span().start().number(),
                kind,
                copy: derives_copy(file, symbol.span()),
                holds: BTreeSet::new(),
                traits: BTreeSet::new(),
            });
        }
    }
    let mut by_name: BTreeMap<String, Vec<TypeIx>> = BTreeMap::new();
    for (ix, node) in model.types.iter().enumerate() {
        by_name.entry(node.name.clone()).or_default().push(ix);
    }
    let types_snapshot = model.types.clone();
    let imports: BTreeMap<String, BTreeMap<String, String>> = index
        .files()
        .filter(|file| is_rust(file))
        .map(|file| (file.path().as_str().to_owned(), use_table(file)))
        .collect();
    let resolver = Resolver {
        imports: &imports,
        by_name: &by_name,
        types: &types_snapshot,
    };
    for entry in index.file_entries() {
        let file = entry.item;
        if !is_rust(file) {
            continue;
        }
        let path = file.path().as_str().to_owned();
        let krate = Unit::Crate.of(&path);
        let impls: Vec<Impl> = file
            .symbols()
            .filter(|symbol| kind_of(symbol).contains("impl"))
            .filter_map(|symbol| {
                let mut found = parse_impl(symbol.name().as_str())?;
                found.span = symbol.span();
                Some(found)
            })
            .collect();
        for found in &impls {
            if let (Some(trait_name), Some(ix)) =
                (&found.trait_name, resolver.resolve(&found.ty, file))
                && let Some(node) = model.types.get_mut(ix)
            {
                node.traits.insert(trait_name.clone());
            }
        }
        for (id, symbol) in symbols_of(entry.id) {
            let kind = kind_of(symbol);
            if let Some(ix) = model.type_of.get(&id).copied() {
                let text = symbol_text(file, symbol.span());
                let toks = tokens(&text);
                let start = toks
                    .iter()
                    .position(|tok| {
                        matches!(tok, Tok::Ident(word) if word == "struct" || word == "enum" || word == "type" || word == "trait")
                    })
                    .map_or(0, |at| at + 2);
                let own = symbol.name().as_str();
                let mut holds = BTreeSet::new();
                for tok in toks.get(start..).unwrap_or_default() {
                    if let Tok::Ident(word) = tok
                        && is_type_word(word)
                        && word != own
                        && let Some(other) = resolver.resolve(word, file)
                        && other != ix
                    {
                        holds.insert(other);
                    }
                }
                if let Some(node) = model.types.get_mut(ix) {
                    node.holds = holds;
                }
                continue;
            }
            if !is_fn(&kind) {
                continue;
            }
            let span = symbol.span();
            let within = impls
                .iter()
                .find(|found| found.span.start() <= span.start() && span.end() <= found.span.end());
            let owner_name = within.map(|found| found.ty.clone());
            let owner = owner_name
                .as_deref()
                .and_then(|name| resolver.resolve(name, file));
            let text = symbol_text(file, span);
            let toks = tokens(&text);
            let Some(fn_at) = toks
                .iter()
                .position(|tok| *tok == Tok::Ident("fn".to_owned()))
            else {
                continue;
            };
            let mut at = fn_at + 2;
            if toks.get(at) == Some(&Tok::Punct('<')) {
                at = matching(&toks, at, '<', '>') + 1;
            }
            if toks.get(at) != Some(&Tok::Punct('(')) {
                continue;
            }
            let close = matching(&toks, at, '(', ')');
            let params = toks.get(at + 1..close).unwrap_or_default();
            let mut uses: BTreeMap<TypeIx, BTreeSet<Access>> = BTreeMap::new();
            let mut add = |ty: TypeIx, access: Access| {
                let copy = types_snapshot.get(ty).is_some_and(|node| node.copy);
                let access = if copy && access == Access::Consume {
                    Access::Read
                } else {
                    access
                };
                uses.entry(ty).or_default().insert(access);
            };
            let resolve_word = |word: &str| {
                if word == "Self" {
                    owner
                } else if is_type_word(word) {
                    resolver.resolve(word, file)
                } else {
                    None
                }
            };
            for param in split_top(params) {
                let is_self = param
                    .iter()
                    .any(|tok| *tok == Tok::Ident("self".to_owned()));
                if is_self {
                    if let Some(owner) = owner {
                        let access = if param.first() == Some(&Tok::Punct('&')) {
                            access_of(param)
                        } else {
                            Access::Consume
                        };
                        add(owner, access);
                    }
                    continue;
                }
                let Some(colon) = param.iter().position(|tok| *tok == Tok::Punct(':')) else {
                    continue;
                };
                let ty = param.get(colon + 1..).unwrap_or_default();
                let access = access_of(ty);
                for tok in ty {
                    if let Tok::Ident(word) = tok
                        && let Some(found) = resolve_word(word)
                    {
                        add(found, access);
                    }
                }
            }
            let body_at = toks
                .iter()
                .enumerate()
                .skip(close)
                .find(|(_, tok)| matches!(tok, Tok::Punct('{' | ';')))
                .map_or(toks.len(), |(at, _)| at);
            let ret = toks.get(close + 1..body_at).unwrap_or_default();
            let lent = ret.iter().any(|tok| *tok == Tok::Punct('&'));
            if ret.first() == Some(&Tok::Punct('-')) {
                for tok in ret {
                    if let Tok::Ident(word) = tok {
                        if word == "where" {
                            break;
                        }
                        if let Some(found) = resolve_word(word) {
                            add(found, if lent { Access::Lend } else { Access::Return });
                        }
                    }
                }
            }
            let body = toks.get(body_at..).unwrap_or_default();
            for at in 0..body.len() {
                let Some(Tok::Ident(word)) = body.get(at) else {
                    continue;
                };
                let Some(found) = resolve_word(word) else {
                    continue;
                };
                let is_enum = types_snapshot
                    .get(found)
                    .is_some_and(|node| node.kind == TypeKind::Enum);
                let second = body.get(at + 1);
                let literal = matches!(second, Some(Tok::Punct('{' | '(')));
                let variant = is_enum
                    && second == Some(&Tok::Punct(':'))
                    && body.get(at + 2) == Some(&Tok::Punct(':'))
                    && matches!(body.get(at + 3), Some(Tok::Ident(name)) if is_type_word(name));
                if !(literal || variant) {
                    continue;
                }
                let mut after = if variant { at + 4 } else { at + 1 };
                if let Some(Tok::Punct(open @ ('(' | '{'))) = body.get(after) {
                    let close = if *open == '(' { ')' } else { '}' };
                    after = matching(body, after, *open, close) + 1;
                }
                let next = body.get(after);
                let then = body.get(after + 1);
                let arm = next == Some(&Tok::Punct('='))
                    && (then == Some(&Tok::Punct('>')) || then != Some(&Tok::Punct('=')));
                let alternative = next == Some(&Tok::Punct('|'));
                let in_matches = body
                    .get(at.saturating_sub(10)..at)
                    .unwrap_or_default()
                    .windows(2)
                    .any(|pair| {
                        pair.first() == Some(&Tok::Ident("matches".to_owned()))
                            && pair.get(1) == Some(&Tok::Punct('!'))
                    });
                let preceded = at > 0
                    && matches!(body.get(at - 1), Some(Tok::Punct('|')))
                    && !matches!(body.get(at.saturating_sub(2)), Some(Tok::Punct('|')));
                if arm || alternative || in_matches || preceded {
                    add(found, Access::Read);
                } else {
                    add(found, Access::Construct);
                }
            }
            let fn_ix = model.fns.len();
            for ty in uses.keys() {
                model.touching.entry(*ty).or_default().push(fn_ix);
            }
            model.fn_of.insert(id, fn_ix);
            model.fns.push(FnNode {
                symbol: id,
                name: symbol.name().as_str().to_owned(),
                owner,
                owner_name,
                krate: krate.clone(),
                span,
                uses,
            });
        }
    }
    for (ix, node) in model.types.iter().enumerate() {
        for held in &node.holds {
            model.held_by.entry(*held).or_default().insert(ix);
        }
    }
    model
}

#[derive(Clone, Debug)]
pub(crate) struct Neighbour {
    pub(crate) ty: TypeIx,
    pub(crate) via: Vec<FnIx>,
}

impl TypeModel {
    pub(crate) fn made_from(&self, focus: TypeIx) -> Vec<Neighbour> {
        self.lineage(focus, true)
    }

    pub(crate) fn turned_into(&self, focus: TypeIx) -> Vec<Neighbour> {
        self.lineage(focus, false)
    }

    fn lineage(&self, focus: TypeIx, upstream: bool) -> Vec<Neighbour> {
        let mut found: BTreeMap<TypeIx, Vec<FnIx>> = BTreeMap::new();
        for fn_ix in self.touching.get(&focus).into_iter().flatten() {
            let Some(node) = self.fns.get(*fn_ix) else {
                continue;
            };
            let (has, others): (bool, Vec<TypeIx>) = if upstream {
                (
                    node.outputs().any(|ty| ty == focus),
                    node.inputs().filter(|ty| *ty != focus).collect(),
                )
            } else {
                (
                    node.inputs().any(|ty| ty == focus),
                    node.outputs().filter(|ty| *ty != focus).collect(),
                )
            };
            if !has {
                continue;
            }
            for other in others {
                found.entry(other).or_default().push(*fn_ix);
            }
        }
        let mut out: Vec<Neighbour> = found
            .into_iter()
            .map(|(ty, via)| Neighbour { ty, via })
            .collect();
        out.sort_by(|a, b| b.via.len().cmp(&a.via.len()).then(a.ty.cmp(&b.ty)));
        out
    }

    pub(crate) fn accesses(&self, focus: TypeIx) -> BTreeMap<Access, usize> {
        let mut counts = BTreeMap::new();
        for fn_ix in self.touching.get(&focus).into_iter().flatten() {
            if let Some(node) = self.fns.get(*fn_ix)
                && let Some(accesses) = node.uses.get(&focus)
            {
                for access in accesses {
                    *counts.entry(*access).or_insert(0) += 1;
                }
            }
        }
        counts
    }

    pub(crate) fn fns_within(&self, file: domain::FileId, span: Span) -> Vec<FnIx> {
        self.fns
            .iter()
            .enumerate()
            .filter(|(_, node)| {
                node.symbol.file() == file
                    && node.span.start() <= span.end()
                    && span.start() <= node.span.end()
            })
            .map(|(ix, _)| ix)
            .collect()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TypesState {
    pub(crate) focus: Option<SymbolId>,
    pub(crate) trail: Vec<SymbolId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TypesAction {
    Focus(SymbolId),
}

impl TypesState {
    pub(crate) fn apply(&mut self, action: TypesAction) {
        match action {
            TypesAction::Focus(symbol) => {
                self.focus = Some(symbol);
                if self.trail.last() != Some(&symbol) {
                    self.trail.retain(|held| *held != symbol);
                    self.trail.push(symbol);
                    if self.trail.len() > 10 {
                        self.trail.remove(0);
                    }
                }
            }
        }
    }
}
