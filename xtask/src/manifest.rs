use std::collections::BTreeMap;
use std::fs;

use crate::text::{Content, CrateName, Literal, Message, RepoPath, Root};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Section(String);

impl Section {
    fn new(header: &str) -> Self {
        Self(header.trim().to_owned())
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Manifest {
    sections: BTreeMap<Section, Vec<Message>>,
}

impl Manifest {
    pub(crate) fn parse(text: &Content) -> Self {
        let mut manifest = Self::default();
        let mut current = Section::new("");
        for line in text.as_str().lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line.starts_with('[') {
                current = Section::new(line);
                manifest.sections.entry(current.clone()).or_default();
                continue;
            }
            manifest
                .sections
                .entry(current.clone())
                .or_default()
                .push(Message::new(line));
        }
        manifest
    }

    pub(crate) fn load(root: &Root, path: &RepoPath) -> Result<Self, Message> {
        fs::read_to_string(root.join(path))
            .map(|text| Self::parse(&Content::new(text)))
            .map_err(|error| Message::new(format!("{path}: {error}")))
    }

    pub(crate) fn package(&self) -> Option<CrateName> {
        self.value(Literal::new("[package]"), Literal::new("name"))
            .map(|name| CrateName::new(name.as_str()))
    }

    pub(crate) fn dependencies(&self) -> Vec<CrateName> {
        self.keys(Literal::new("[dependencies]"))
    }

    pub(crate) fn members(&self, root: &Root) -> Vec<RepoPath> {
        let Some(lines) = self.sections.get(&Section::new("[workspace]")) else {
            return Vec::new();
        };
        let joined: String = lines
            .iter()
            .map(Message::as_str)
            .collect::<Vec<_>>()
            .join(" ");
        let Some((_, list)) = joined.split_once("members") else {
            return Vec::new();
        };
        let inside = list
            .split_once('[')
            .and_then(|(_, rest)| rest.split_once(']'))
            .map(|(inside, _)| inside)
            .unwrap_or_default();
        let mut out = Vec::new();
        for member in inside
            .split(',')
            .map(|member| member.trim().trim_matches('"'))
            .filter(|member| !member.is_empty())
        {
            match member.strip_suffix("/*") {
                Some(folder) => out.extend(member_folders(root, &RepoPath::new(folder))),
                None => out.push(RepoPath::new(member)),
            }
        }
        out
    }

    pub(crate) fn guarded(&self) -> BTreeMap<Section, Vec<Message>> {
        self.sections
            .iter()
            .filter(|(section, _)| {
                let name = section.as_str();
                name.starts_with("[workspace")
                    || name.starts_with("[lints")
                    || name.starts_with("[dependencies")
                    || name.starts_with("[dev-dependencies")
                    || name.starts_with("[build-dependencies")
            })
            .map(|(section, lines)| (section.clone(), lines.clone()))
            .collect()
    }

    fn keys(&self, header: Literal) -> Vec<CrateName> {
        self.sections
            .get(&Section::new(header.as_str()))
            .map(|lines| {
                lines
                    .iter()
                    .filter_map(|line| line.as_str().split_once('='))
                    .map(|(key, _)| CrateName::new(key))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn value(&self, header: Literal, key: Literal) -> Option<Message> {
        self.sections
            .get(&Section::new(header.as_str()))?
            .iter()
            .find_map(|line| {
                let (name, value) = line.as_str().split_once('=')?;
                (name.trim() == key.as_str()).then(|| Message::new(value.trim().trim_matches('"')))
            })
    }
}

fn member_folders(root: &Root, folder: &RepoPath) -> Vec<RepoPath> {
    let mut out: Vec<RepoPath> = fs::read_dir(root.join(folder))
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|entry| entry.path().join("Cargo.toml").is_file())
                .filter_map(|entry| root.relative(&entry.path()))
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}
