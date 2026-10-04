use std::mem;

use crate::answer::{Fragment, HoverLine, Inline};
use crate::wire::WireName;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FenceMark {
    Backticks,
    Tildes,
}

impl FenceMark {
    const ALL: [Self; 2] = [Self::Backticks, Self::Tildes];

    const fn name(self) -> WireName {
        WireName::new(match self {
            Self::Backticks => "```",
            Self::Tildes => "~~~",
        })
    }

    const fn glyph(self) -> Glyph {
        Glyph(match self {
            Self::Backticks => '`',
            Self::Tildes => '~',
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Block {
    Prose,
    Fenced(FenceMark),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Delimiter {
    Tick,
    OpenBracket,
    CloseBracket,
    OpenParen,
    CloseParen,
    Escape,
    Star,
    Underscore,
    Hash,
    Bang,
    Colon,
    Dash,
}

impl Delimiter {
    const INLINE: [Self; 6] = [
        Self::Tick,
        Self::OpenBracket,
        Self::Escape,
        Self::Star,
        Self::Underscore,
        Self::Bang,
    ];
    const RULE: [Self; 3] = [Self::Dash, Self::Star, Self::Underscore];

    const fn glyph(self) -> Glyph {
        Glyph(match self {
            Self::Tick => '`',
            Self::OpenBracket => '[',
            Self::CloseBracket => ']',
            Self::OpenParen => '(',
            Self::CloseParen => ')',
            Self::Escape => '\\',
            Self::Star => '*',
            Self::Underscore => '_',
            Self::Hash => '#',
            Self::Bang => '!',
            Self::Colon => ':',
            Self::Dash => '-',
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Glyph(char);

impl Glyph {
    const fn get(self) -> char {
        self.0
    }

    fn is(self, glyph: Option<&char>) -> bool {
        glyph.is_some_and(|glyph| *glyph == self.0)
    }

    fn delimiter(glyph: Option<&char>) -> Option<Delimiter> {
        Delimiter::INLINE
            .into_iter()
            .find(|delimiter| delimiter.glyph().is(glyph))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Opener(String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Markdown(String);

impl Markdown {
    pub(crate) fn new(text: &str) -> Self {
        Self(text.to_owned())
    }

    pub(crate) fn lines(&self) -> Vec<HoverLine> {
        let mut lines = Vec::new();
        let mut block = Block::Prose;
        for line in self.0.lines() {
            let opened = line.trim_start();
            let mark = FenceMark::ALL.into_iter().find(|mark| {
                opened.starts_with(mark.name().as_str())
                    && !opened
                        .trim_start_matches(mark.glyph().get())
                        .contains(mark.glyph().get())
            });
            block = match (block, mark) {
                (Block::Prose, Some(mark)) => Block::Fenced(mark),
                (Block::Fenced(open), Some(mark)) if open == mark => Block::Prose,
                (Block::Fenced(open), _) => {
                    lines.push(HoverLine::Fenced(Fragment::new(line.trim_end())));
                    Block::Fenced(open)
                }
                (Block::Prose, None) => {
                    lines.push(Self::block(line.trim_end()));
                    Block::Prose
                }
            };
        }
        lines
    }

    fn block(line: &str) -> HoverLine {
        let trimmed = line.trim();
        if trimmed.is_empty() || Self::is_reference_definition(trimmed) {
            return HoverLine::Blank;
        }
        if Self::is_rule(trimmed) {
            return HoverLine::Rule;
        }
        let hash = Delimiter::Hash.glyph().get();
        let hashes = trimmed.chars().take_while(|glyph| *glyph == hash).count();
        let rest: String = trimmed.chars().skip(hashes).collect();
        if (1..=6).contains(&hashes) && (rest.is_empty() || rest.starts_with(' ')) {
            let title = rest.trim().trim_end_matches(hash).trim_end();
            return HoverLine::Heading(Self::inlines(title));
        }
        HoverLine::Prose(Self::inlines(line))
    }

    fn is_rule(trimmed: &str) -> bool {
        let glyphs: Vec<char> = trimmed
            .chars()
            .filter(|glyph| !glyph.is_whitespace())
            .collect();
        let Some(first) = glyphs.first() else {
            return false;
        };
        Delimiter::RULE
            .iter()
            .any(|mark| mark.glyph().is(Some(first)))
            && glyphs.len() >= 3
            && glyphs.iter().all(|glyph| glyph == first)
    }

    fn is_reference_definition(trimmed: &str) -> bool {
        let glyphs: Vec<char> = trimmed.chars().collect();
        Delimiter::OpenBracket.glyph().is(glyphs.first())
            && Self::closing(&glyphs, 0, Delimiter::OpenBracket, Delimiter::CloseBracket)
                .is_some_and(|end| Delimiter::Colon.glyph().is(glyphs.get(end + 1)))
    }

    fn inlines(text: &str) -> Vec<Inline> {
        let glyphs: Vec<char> = text.chars().collect();
        let mut builder = InlineBuilder::default();
        let mut at = 0;
        while at < glyphs.len() {
            at = match Glyph::delimiter(glyphs.get(at)) {
                Some(Delimiter::Escape)
                    if glyphs.get(at + 1).is_some_and(char::is_ascii_punctuation) =>
                {
                    Self::prose(&mut builder, &glyphs, at + 1, at + 2);
                    at + 2
                }
                Some(Delimiter::Tick) => Self::code_span(&glyphs, at, &mut builder),
                Some(Delimiter::Bang) if Delimiter::OpenBracket.glyph().is(glyphs.get(at + 1)) => {
                    Self::link_label(&glyphs, at + 1, &mut builder).unwrap_or_else(|| {
                        Self::prose(&mut builder, &glyphs, at, at + 1);
                        at + 1
                    })
                }
                Some(Delimiter::OpenBracket) => Self::link_label(&glyphs, at, &mut builder)
                    .unwrap_or_else(|| {
                        Self::prose(&mut builder, &glyphs, at, at + 1);
                        at + 1
                    }),
                Some(delimiter @ (Delimiter::Star | Delimiter::Underscore)) => {
                    Self::emphasis(&glyphs, at, delimiter.glyph(), &mut builder)
                }
                _ => {
                    Self::prose(&mut builder, &glyphs, at, at + 1);
                    at + 1
                }
            };
        }
        builder.finish()
    }

    fn prose(builder: &mut InlineBuilder, glyphs: &[char], start: usize, end: usize) {
        builder.prose(&Fragment::new(&Self::between(glyphs, start, end)));
    }

    fn between(glyphs: &[char], start: usize, end: usize) -> String {
        glyphs
            .iter()
            .skip(start)
            .take(end.saturating_sub(start))
            .collect()
    }

    fn run_length(glyphs: &[char], at: usize, glyph: Glyph) -> usize {
        glyphs
            .iter()
            .skip(at)
            .take_while(|other| glyph.is(Some(other)))
            .count()
    }

    fn code_span(glyphs: &[char], at: usize, builder: &mut InlineBuilder) -> usize {
        let tick = Delimiter::Tick.glyph();
        let ticks = Self::run_length(glyphs, at, tick);
        let mut look = at + ticks;
        while look < glyphs.len() {
            let closing = Self::run_length(glyphs, look, tick);
            if closing == ticks {
                let code = Self::between(glyphs, at + ticks, look);
                builder.code(&Fragment::new(code.trim()));
                return look + ticks;
            }
            look += closing.max(1);
        }
        Self::prose(builder, glyphs, at, at + ticks);
        at + ticks
    }

    fn closing(glyphs: &[char], at: usize, open: Delimiter, close: Delimiter) -> Option<usize> {
        let mut depth = 0_usize;
        let mut look = at;
        while let Some(glyph) = glyphs.get(look) {
            if Delimiter::Tick.glyph().is(Some(glyph)) {
                look += Self::run_length(glyphs, look, Delimiter::Tick.glyph());
                continue;
            }
            if Delimiter::Escape.glyph().is(Some(glyph)) {
                look += 2;
                continue;
            }
            if open.glyph().is(Some(glyph)) {
                depth += 1;
            } else if close.glyph().is(Some(glyph)) {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(look);
                }
            }
            look += 1;
        }
        None
    }

    fn link_label(glyphs: &[char], at: usize, builder: &mut InlineBuilder) -> Option<usize> {
        let label_end = Self::closing(glyphs, at, Delimiter::OpenBracket, Delimiter::CloseBracket)?;
        let next = label_end + 1;
        let target = if Delimiter::OpenParen.glyph().is(glyphs.get(next)) {
            Self::closing(glyphs, next, Delimiter::OpenParen, Delimiter::CloseParen)?
        } else if Delimiter::OpenBracket.glyph().is(glyphs.get(next)) {
            Self::closing(
                glyphs,
                next,
                Delimiter::OpenBracket,
                Delimiter::CloseBracket,
            )?
        } else {
            return None;
        };
        builder.extend(Self::inlines(&Self::between(glyphs, at + 1, label_end)));
        Some(target + 1)
    }

    fn flanks(glyphs: &[char], at: usize, count: usize, glyph: Glyph) -> (bool, bool) {
        let before = at.checked_sub(1).and_then(|index| glyphs.get(index));
        let after = glyphs.get(at + count);
        let word = |side: Option<&char>| side.is_some_and(|other| other.is_alphanumeric());
        let space = |side: Option<&char>| side.is_none_or(|other| other.is_whitespace());
        let underscore = Delimiter::Underscore.glyph() == glyph;
        let opens = !space(after) && (!underscore || !word(before));
        let closes = !space(before) && (!underscore || !word(after));
        (opens, closes)
    }

    fn closer_ahead(glyphs: &[char], from: usize, count: usize, glyph: Glyph) -> bool {
        let mut look = from;
        while look < glyphs.len() {
            let found = Self::run_length(glyphs, look, glyph);
            if found == 0 {
                look += 1;
                continue;
            }
            if found == count && Self::flanks(glyphs, look, found, glyph).1 {
                return true;
            }
            look += found;
        }
        false
    }

    fn emphasis(glyphs: &[char], at: usize, glyph: Glyph, builder: &mut InlineBuilder) -> usize {
        let count = Self::run_length(glyphs, at, glyph);
        let run = Opener(Self::between(glyphs, at, at + count));
        let (opens, closes) = Self::flanks(glyphs, at, count, glyph);
        if closes && builder.openers.last() == Some(&run) {
            builder.openers.pop();
        } else if opens && Self::closer_ahead(glyphs, at + count, count, glyph) {
            builder.openers.push(run);
        } else {
            Self::prose(builder, glyphs, at, at + count);
        }
        at + count
    }
}

#[derive(Debug, Default)]
struct InlineBuilder {
    done: Vec<Inline>,
    prose: Fragment,
    openers: Vec<Opener>,
}

impl InlineBuilder {
    fn prose(&mut self, text: &Fragment) {
        self.prose.push(text.as_str());
    }

    fn flush(&mut self) {
        let prose = mem::take(&mut self.prose);
        if !prose.as_str().is_empty() {
            self.done.push(Inline::Prose(prose));
        }
    }

    fn code(&mut self, code: &Fragment) {
        self.flush();
        self.done.push(Inline::Code(code.clone()));
    }

    fn extend(&mut self, inlines: Vec<Inline>) {
        for inline in inlines {
            match inline {
                Inline::Prose(text) => self.prose(&text),
                Inline::Code(text) => self.code(&text),
            }
        }
    }

    fn finish(mut self) -> Vec<Inline> {
        self.flush();
        self.done
    }
}
