use std::collections::BTreeMap;

use crate::lint::checks::named_by_us;
use crate::source::{SourceFile, Zone, line_of, rust_sources};
use crate::text::{Count, Message, Root, Word};
use crate::vocabulary::{Verdict, Vocabulary};

pub(crate) fn unknown(root: &Root) -> Result<Message, Message> {
    let vocabulary = Vocabulary::load(root).unwrap_or_default();
    let mut seen: BTreeMap<Word, (Count, Message)> = BTreeMap::new();
    for path in rust_sources(root) {
        let file = SourceFile::load(root, path)?;
        if file.zone != Zone::Strict {
            continue;
        }
        for name in named_by_us(&file) {
            {
                let identifier = file.text.of(name);
                for word in Word::split(identifier) {
                    if vocabulary.verdict(&word) == Verdict::Known {
                        continue;
                    }
                    let entry = seen.entry(word).or_insert_with(|| {
                        (
                            Count::ZERO,
                            Message::new(format!("{}:{} {identifier}", file.path, line_of(name))),
                        )
                    });
                    entry.0 = entry.0.next();
                }
            }
        }
    }
    let mut report = Message::default();
    for (word, (count, first)) in &seen {
        report.push_line(&format!("{word} {count} {first}"));
    }
    Ok(report)
}
