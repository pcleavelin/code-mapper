use crate::index::{File, Index, SymRef};
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Author {
    Human = 0,
    Ai = 1,
}

impl Author {
    pub fn tag(self) -> &'static str {
        match self {
            Author::Human => "",
            Author::Ai => " (ai)",
        }
    }
    fn from_u8(b: u8) -> Option<Author> {
        match b {
            0 => Some(Author::Human),
            1 => Some(Author::Ai),
            _ => None,
        }
    }
}

/// Pins a slice of lines. Offsets are relative to the start of the enclosing symbol so the anchor
/// survives edits elsewhere in the file. `symbol == ""` means absolute lines. `hash` detects when
/// the anchored text itself changed (-> stale).
pub struct Anchor {
    pub file: String,
    pub symbol: String,
    pub off_start: i32,
    pub off_end: i32,
    pub hash: u64,
    pub author: Author,
    pub note: String,

    // resolved per session, not stored
    pub line_start: usize,
    pub line_end: usize,
    pub stale: bool,
}

pub struct PathDef {
    pub name: String,
    pub note: String,
    pub author: Author,
    pub anchors: Vec<Anchor>,
}

#[derive(Default)]
pub struct Map {
    pub paths: Vec<PathDef>,
}

// ---- binary file format ----------------------------------------------------------
// "CMAP" u32 version
// u32 npaths { str name, str note, u8 author, u32 nanchors {
//     str file, str symbol, i32 off_start, i32 off_end, u64 hash, u8 author, str note (v3+) } }
// str = u32 len + utf8 bytes. All little-endian. v2 files (no anchor note) are still read.

const MAGIC: &[u8; 4] = b"CMAP";
const VERSION: u32 = 3;

fn w_str(b: &mut Vec<u8>, s: &str) {
    b.extend_from_slice(&(s.len() as u32).to_le_bytes());
    b.extend_from_slice(s.as_bytes());
}

struct Reader<'a> {
    data: &'a [u8],
    off: usize,
}

impl Reader<'_> {
    fn bytes(&mut self, n: usize) -> Option<&[u8]> {
        let s = self.data.get(self.off..self.off + n)?;
        self.off += n;
        Some(s)
    }
    fn u8(&mut self) -> Option<u8> {
        Some(self.bytes(1)?[0])
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.bytes(4)?.try_into().ok()?))
    }
    fn i32(&mut self) -> Option<i32> {
        Some(i32::from_le_bytes(self.bytes(4)?.try_into().ok()?))
    }
    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.bytes(8)?.try_into().ok()?))
    }
    fn str(&mut self) -> Option<String> {
        let n = self.u32()? as usize;
        Some(String::from_utf8_lossy(self.bytes(n)?).into_owned())
    }
}

impl Map {
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let mut b = Vec::with_capacity(4096);
        b.extend_from_slice(MAGIC);
        b.extend_from_slice(&VERSION.to_le_bytes());
        b.extend_from_slice(&(self.paths.len() as u32).to_le_bytes());
        for p in &self.paths {
            w_str(&mut b, &p.name);
            w_str(&mut b, &p.note);
            b.push(p.author as u8);
            b.extend_from_slice(&(p.anchors.len() as u32).to_le_bytes());
            for a in &p.anchors {
                w_str(&mut b, &a.file);
                w_str(&mut b, &a.symbol);
                b.extend_from_slice(&a.off_start.to_le_bytes());
                b.extend_from_slice(&a.off_end.to_le_bytes());
                b.extend_from_slice(&a.hash.to_le_bytes());
                b.push(a.author as u8);
                w_str(&mut b, &a.note);
            }
        }
        std::fs::write(path, b)
    }

    pub fn load(path: &Path) -> Option<Map> {
        let data = std::fs::read(path).ok()?;
        let mut r = Reader { data: &data, off: 0 };
        if r.bytes(4)? != MAGIC {
            return None;
        }
        let version = r.u32()?;
        if version != 2 && version != VERSION {
            return None;
        }
        let mut m = Map::default();
        for _ in 0..r.u32()? {
            let mut p = PathDef {
                name: r.str()?,
                note: r.str()?,
                author: Author::from_u8(r.u8()?)?,
                anchors: Vec::new(),
            };
            for _ in 0..r.u32()? {
                p.anchors.push(Anchor {
                    file: r.str()?,
                    symbol: r.str()?,
                    off_start: r.i32()?,
                    off_end: r.i32()?,
                    hash: r.u64()?,
                    author: Author::from_u8(r.u8()?)?,
                    note: if version >= 3 { r.str()? } else { String::new() },
                    line_start: 0,
                    line_end: 0,
                    stale: true,
                });
            }
            m.paths.push(p);
        }
        Some(m)
    }

    // ---- mutations shared by the GUI and the CLI ----

    pub fn find(&self, name: &str) -> Option<usize> {
        self.paths.iter().position(|p| p.name == name)
    }

    /// Returns the existing path of that name, or creates it.
    pub fn add_path(&mut self, name: &str, author: Author) -> usize {
        if let Some(pi) = self.find(name) {
            return pi;
        }
        self.paths.push(PathDef { name: name.to_owned(), note: String::new(), author, anchors: Vec::new() });
        self.paths.len() - 1
    }

    pub fn add_anchor(&mut self, idx: &Index, pi: usize, fi: usize, ls: usize, le: usize, author: Author) {
        let mut a = Anchor::new(&idx.files[fi], ls, le);
        a.author = author;
        self.paths[pi].anchors.push(a);
    }

    /// A path named after `root`, with one anchor per symbol of its call tree in pre-order.
    pub fn promote(&mut self, idx: &Index, root: SymRef, depth: usize, author: Author) -> usize {
        let pi = self.add_path(&idx.sym(root).name.clone(), author);
        for (r, _) in idx.call_tree(root, depth) {
            let s = idx.sym(r);
            let file = &idx.files[r.file].path;
            if self.paths[pi].anchors.iter().any(|a| &a.file == file && a.symbol == s.name) {
                continue; // re-promoting an existing path adds only what is new
            }
            self.add_anchor(idx, pi, r.file, s.start, s.end, author);
        }
        pi
    }

    pub fn resolve_all(&mut self, idx: &Index) {
        for p in &mut self.paths {
            for a in &mut p.anchors {
                a.resolve(idx);
            }
        }
    }
}

// ---- anchoring --------------------------------------------------------------------

pub fn slice_hash(lines: &[String], ls: usize, le: usize) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for line in &lines[ls..=le] {
        for b in line.bytes().chain(std::iter::once(b'\n')) {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    h
}

impl Anchor {
    pub fn new(f: &File, ls: usize, le: usize) -> Anchor {
        let mut a = Anchor {
            file: f.path.clone(),
            symbol: String::new(),
            off_start: ls as i32,
            off_end: le as i32,
            hash: slice_hash(&f.lines, ls, le),
            author: Author::Human,
            note: String::new(),
            line_start: ls,
            line_end: le,
            stale: false,
        };
        // innermost symbol containing the slice
        if let Some(s) = f.symbols.iter().filter(|s| s.start <= ls && le <= s.end).max_by_key(|s| s.depth) {
            a.symbol = s.name.clone();
            a.off_start = (ls - s.start) as i32;
            a.off_end = (le - s.start) as i32;
        }
        a
    }

    pub fn resolve(&mut self, idx: &Index) {
        self.stale = true;
        let Some(f) = idx.find_file(&self.file).map(|i| &idx.files[i]) else { return };

        let base = if self.symbol.is_empty() {
            0
        } else {
            match f.symbols.iter().find(|s| s.name == self.symbol) {
                Some(s) => s.start as i64,
                None => return,
            }
        };
        let ls = base + self.off_start as i64;
        let le = base + self.off_end as i64;
        if ls < 0 || le >= f.lines.len() as i64 || ls > le {
            return;
        }
        self.line_start = ls as usize;
        self.line_end = le as usize;
        self.stale = slice_hash(&f.lines, self.line_start, self.line_end) != self.hash;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::Symbol;

    fn sym(name: &str, start: usize, end: usize) -> Symbol {
        Symbol { name: name.into(), kind: "fn", start, end, depth: 0, calls: vec![], callees: vec![], callers: vec![] }
    }

    #[test]
    fn round_trip_and_stale() {
        let lines: Vec<String> = ["fn a() {", "  1", "}", "fn b() {", "  2", "}"].map(String::from).to_vec();
        let file = File { path: "a.rs".into(), hl: vec![Vec::new(); lines.len()], lines, symbols: vec![sym("b", 3, 5)], mtime: None };
        let idx = Index { root: ".".into(), files: vec![file] };
        let mut m = Map::default();
        let pi = m.add_path("p", Author::Ai);
        m.add_anchor(&idx, pi, 0, 4, 4, Author::Ai);
        m.paths[0].anchors[0].note = "the middle".into();
        assert_eq!(m.paths[0].anchors[0].symbol, "b");
        assert_eq!(m.paths[0].anchors[0].off_start, 1);

        let tmp = std::env::temp_dir().join("codemap_test.cmap");
        m.save(&tmp).unwrap();
        let mut loaded = Map::load(&tmp).unwrap();
        assert_eq!(loaded.paths[0].author, Author::Ai);
        assert_eq!(loaded.paths[0].anchors[0].author, Author::Ai);
        assert_eq!(loaded.paths[0].anchors[0].note, "the middle");

        // symbol `b` moved down two lines: anchor follows it and is not stale
        let mut idx = idx;
        idx.files[0].lines.splice(0..0, ["// x".to_string(), "// y".to_string()]);
        idx.files[0].symbols[0].start += 2;
        idx.files[0].symbols[0].end += 2;
        loaded.resolve_all(&idx);
        assert_eq!(loaded.paths[0].anchors[0].line_start, 6);
        assert!(!loaded.paths[0].anchors[0].stale);

        // text changed: stale
        idx.files[0].lines[6] = "  3".into();
        loaded.resolve_all(&idx);
        assert!(loaded.paths[0].anchors[0].stale);
    }
}
