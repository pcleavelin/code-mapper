use super::{Backend, Call, File, Index, Qual, Span, Symbol};
use crate::codec::{Reader, w_str, w_u32, write_retry};
use std::collections::HashMap;
use std::path::Path;

pub const CACHE: &str = ".codemap-cache";
const CACHE_MAGIC: &[u8; 4] = b"CMCH";
const CACHE_VERSION: u32 = 1;

fn w_locs(b: &mut Vec<u8>, locs: &[(String, u32)]) {
    w_u32(b, locs.len() as u32);
    for (p, l) in locs {
        w_str(b, p);
        w_u32(b, *l);
    }
}

fn locs(r: &mut Reader) -> Option<Vec<(String, u32)>> {
    (0..r.u32()?).map(|_| Some((r.str()?, r.u32()?))).collect()
}

pub(super) fn load_cache(path: &Path) -> Option<HashMap<String, File>> {
    let data = std::fs::read(path).ok()?;
    let mut r = Reader {
        data: &data,
        off: 0,
    };
    if r.bytes(4)? != CACHE_MAGIC || r.u32()? != CACHE_VERSION {
        return None;
    }
    let mut out = HashMap::new();
    for _ in 0..r.u32()? {
        let path = r.str()?;
        let hash = r.u64()?;
        let backend = if r.u8()? == 1 {
            Backend::Server
        } else {
            Backend::TreeSitter
        };
        let mut symbols = Vec::new();
        for _ in 0..r.u32()? {
            let (name, kind) = (r.str()?, r.str()?);
            let (start, end, depth) = (r.u32()? as usize, r.u32()? as usize, r.u8()?);
            let owner = Some(r.str()?).filter(|o| !o.is_empty());
            let mut calls = Vec::new();
            for _ in 0..r.u32()? {
                let name = r.str()?;
                let qual = match r.u8()? {
                    0 => Qual::None,
                    1 => Qual::SelfRef,
                    _ => Qual::Some(r.str()?),
                };
                calls.push(Call { name, qual });
            }
            let (targets, refs) = (locs(&mut r)?, locs(&mut r)?);
            symbols.push(Symbol {
                name,
                kind,
                start,
                end,
                depth,
                owner,
                calls,
                targets,
                refs,
                callees: Vec::new(),
                callers: Vec::new(),
            });
        }
        let imports = (0..r.u32()?)
            .map(|_| Some((r.str()?, r.str()?)))
            .collect::<Option<HashMap<_, _>>>()?;
        let mut hl = Vec::new();
        for _ in 0..r.u32()? {
            hl.push(
                (0..r.u32()?)
                    .map(|_| Some((r.u32()?, r.u32()?, r.u8()?)))
                    .collect::<Option<Vec<Span>>>()?,
            );
        }
        out.insert(
            path.clone(),
            File {
                path,
                lines: Vec::new(),
                hl,
                symbols,
                imports,
                mtime: None,
                hash,
                backend,
                pending: false,
            },
        );
    }
    Some(out)
}

impl Index {
    pub fn save_cache(&self) {
        let mut b = Vec::with_capacity(1 << 16);
        b.extend_from_slice(CACHE_MAGIC);
        w_u32(&mut b, CACHE_VERSION);
        w_u32(&mut b, self.files.len() as u32);
        for f in &self.files {
            w_str(&mut b, &f.path);
            b.extend_from_slice(&f.hash.to_le_bytes());
            b.push(f.backend as u8);
            w_u32(&mut b, f.symbols.len() as u32);
            for s in &f.symbols {
                w_str(&mut b, &s.name);
                w_str(&mut b, &s.kind);
                w_u32(&mut b, s.start as u32);
                w_u32(&mut b, s.end as u32);
                b.push(s.depth);
                w_str(&mut b, s.owner.as_deref().unwrap_or(""));
                w_u32(&mut b, s.calls.len() as u32);
                for c in &s.calls {
                    w_str(&mut b, &c.name);
                    match &c.qual {
                        Qual::None => b.push(0),
                        Qual::SelfRef => b.push(1),
                        Qual::Some(q) => {
                            b.push(2);
                            w_str(&mut b, q);
                        }
                    }
                }
                w_locs(&mut b, &s.targets);
                w_locs(&mut b, &s.refs);
            }
            w_u32(&mut b, f.imports.len() as u32);
            let mut imports: Vec<_> = f.imports.iter().collect();
            imports.sort();
            for (k, v) in imports {
                w_str(&mut b, k);
                w_str(&mut b, v);
            }
            w_u32(&mut b, f.hl.len() as u32);
            for spans in &f.hl {
                w_u32(&mut b, spans.len() as u32);
                for &(s, e, c) in spans {
                    w_u32(&mut b, s);
                    w_u32(&mut b, e);
                    b.push(c);
                }
            }
        }
        let _ = write_retry(&self.root.join(CACHE), &b);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index_at(root: &Path, imports: HashMap<String, String>) -> Index {
        let file = File {
            path: "a.rs".into(),
            lines: vec!["fn a() {}".into()],
            hl: vec![Vec::new()],
            symbols: Vec::new(),
            imports,
            mtime: None,
            hash: 1,
            backend: Backend::TreeSitter,
            pending: false,
        };
        Index {
            root: root.to_path_buf(),
            files: vec![file],
        }
    }

    #[test]
    fn the_same_index_writes_the_same_bytes() {
        let names: Vec<(String, String)> = (0..32)
            .map(|n| (format!("name{n}"), format!("module{n}")))
            .collect();
        let forward: HashMap<String, String> = names.iter().cloned().collect();
        let backward: HashMap<String, String> = names.iter().rev().cloned().collect();
        let base = std::env::temp_dir().join(format!("codemap_cache_bytes_{}", std::process::id()));
        let (one, two) = (base.join("one"), base.join("two"));
        for dir in [&one, &two] {
            std::fs::create_dir_all(dir).unwrap();
        }
        index_at(&one, forward).save_cache();
        index_at(&two, backward).save_cache();
        let bytes = |dir: &Path| std::fs::read(dir.join(CACHE)).unwrap();
        assert_eq!(bytes(&one), bytes(&two));
        let _ = std::fs::remove_dir_all(&base);
    }
}
