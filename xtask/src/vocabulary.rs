use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use crate::text::{Content, Literal, Message, RepoPath, Root, Word};

pub(crate) const VOCABULARY_FILE: Literal = Literal::new("vocabulary.txt");

#[derive(Debug, Default)]
pub(crate) struct Vocabulary {
    words: BTreeSet<Word>,
    synonyms: BTreeMap<Word, Word>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Verdict {
    Known,
    Unknown,
    Synonym(Word),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Arrow {
    Points,
}

impl Arrow {
    const fn name(self) -> Literal {
        match self {
            Self::Points => Literal::new("->"),
        }
    }
}

impl Vocabulary {
    pub(crate) fn load(root: &Root) -> Result<Self, Message> {
        let path = RepoPath::new(VOCABULARY_FILE.as_str());
        match fs::read_to_string(root.join(&path)) {
            Ok(text) => Self::parse(&Content::new(text)),
            Err(error) => Err(Message::new(format!("{path}: {error}"))),
        }
    }

    pub(crate) fn parse(text: &Content) -> Result<Self, Message> {
        let mut vocabulary = Self::default();
        for (row, line) in text.as_str().lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match line.split_once(Arrow::Points.name().as_str()) {
                Some((synonym, canonical)) => {
                    vocabulary
                        .synonyms
                        .insert(Word::new(synonym.trim()), Word::new(canonical.trim()));
                }
                None if line.chars().all(|character| {
                    character.is_ascii_lowercase() || character.is_ascii_digit()
                }) =>
                {
                    vocabulary.words.insert(Word::new(line));
                }
                None => {
                    return Err(Message::new(format!(
                        "{VOCABULARY_FILE}:{}: a line is a lowercase word or `synonym -> word`",
                        row + 1
                    )));
                }
            }
        }
        Ok(vocabulary)
    }

    pub(crate) fn verdict(&self, word: &Word) -> Verdict {
        let forms = word.forms();
        if let Some(canonical) = forms.iter().find_map(|form| self.synonyms.get(form)) {
            return Verdict::Synonym(canonical.clone());
        }
        let numeric = word
            .as_str()
            .chars()
            .all(|character| character.is_ascii_digit());
        if numeric || forms.iter().any(|form| self.words.contains(form)) {
            Verdict::Known
        } else {
            Verdict::Unknown
        }
    }

    pub(crate) fn synonym_lines(text: &Content) -> BTreeSet<Message> {
        text.as_str()
            .lines()
            .filter(|line| line.contains(Arrow::Points.name().as_str()))
            .map(|line| Message::new(line.trim()))
            .collect()
    }
}
