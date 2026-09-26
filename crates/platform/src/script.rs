use std::collections::VecDeque;
use std::env;
use std::fmt;
use std::fs;
use std::time::{Duration, Instant};

use ui::{Button, Coordinate, Glyph, Id, Key, Mods, Point, Press, Px};

use crate::report::report;
use crate::window::{App, Exit, Runner};

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ScriptLine(String);

impl ScriptLine {
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn words(&self) -> Vec<&str> {
        self.0.split_whitespace().collect()
    }

    pub fn word(&self, index: usize) -> Option<&str> {
        self.0.split_whitespace().nth(index)
    }

    pub fn number(&self, index: usize) -> i32 {
        self.word(index)
            .and_then(|word| word.parse().ok())
            .unwrap_or(0)
    }

    pub fn rest(&self, from: usize) -> String {
        self.words()
            .into_iter()
            .skip(from)
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn contains_word(&self, wanted: &str) -> bool {
        self.0.split_whitespace().any(|word| word == wanted)
    }

    pub(crate) fn mods_from(&self, from: usize) -> Mods {
        let later: Vec<&str> = self.words().into_iter().skip(from).collect();
        Mods::new(
            later.contains(&"ctrl"),
            later.contains(&"shift"),
            later.contains(&"alt"),
        )
    }

    fn key(&self) -> Key {
        match self.word(1).unwrap_or_default() {
            "enter" => Key::Enter,
            "escape" => Key::Escape,
            "backspace" => Key::Backspace,
            "left" => Key::Left,
            "right" => Key::Right,
            "up" => Key::Up,
            "down" => Key::Down,
            other => Key::Character(Glyph::new(other.chars().next().unwrap_or(' '))),
        }
    }

    fn point(&self, from: usize) -> Point {
        Point::new(Px::new(self.number(from)), Px::new(self.number(from + 1)))
    }

    pub(crate) fn click(&self) -> Vec<Self> {
        let mut rest = self.rest(3);
        if self.word(0) == Some("dblclick") {
            rest.push_str(" twice");
        }
        vec![
            Self(format!(
                "mouse {} {}",
                self.word(1).unwrap_or_default(),
                self.word(2).unwrap_or_default()
            )),
            Self(format!("down {rest}")),
            Self::new("wait 1"),
            Self::new("up"),
            Self::new("wait 1"),
        ]
    }

    pub(crate) fn aimed(&self, at: Point) -> Self {
        let command = self.word(0).unwrap_or_default();
        let command_name = match command.strip_suffix("-id").unwrap_or(command) {
            "hover" => "mouse",
            other => other,
        };
        Self(format!(
            "{command_name} {} {} {}",
            at.horizontal,
            at.vertical,
            self.rest(2)
        ))
    }

    pub(crate) fn drag(&self) -> Vec<Self> {
        let from = self.point(1);
        let to = self.point(3);
        let steps = 8;
        let mut lines = vec![
            Self(format!("mouse {} {}", from.horizontal, from.vertical)),
            Self::new("down"),
            Self::new("wait 1"),
        ];
        for step in 1..=steps {
            let horizontal = from.horizontal.get()
                + (to.horizontal.get() - from.horizontal.get()) * step / steps;
            let vertical =
                from.vertical.get() + (to.vertical.get() - from.vertical.get()) * step / steps;
            lines.push(Self(format!("mouse {horizontal} {vertical}")));
            lines.push(Self::new("wait 1"));
        }
        lines.push(Self::new("up"));
        lines.push(Self::new("wait 1"));
        lines
    }

    fn wait_frames(&self) -> WaitFrames {
        WaitFrames(u32::try_from(self.number(1).max(1) - 1).unwrap_or(0))
    }

    fn pause(&self) -> Duration {
        Duration::from_millis(u64::try_from(self.number(1).max(0)).unwrap_or(0))
    }

    fn element(&self) -> Option<Id> {
        self.word(1).map(Id::from_name)
    }
}

impl fmt::Display for ScriptLine {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
struct WaitFrames(u32);

impl WaitFrames {
    const fn is_zero(self) -> bool {
        self.0 == 0
    }

    const fn previous(self) -> Self {
        Self(self.0.saturating_sub(1))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    Done,
    Retry,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Flow {
    Next,
    Yield,
    Quit,
}

pub(crate) struct Script {
    lines: VecDeque<ScriptLine>,
    wait: WaitFrames,
    until: Option<Instant>,
}

impl Script {
    pub(crate) fn load() -> Option<Self> {
        let path = env::var_os("CODEMAP_SCRIPT")?;
        let text = fs::read_to_string(&path).ok()?;
        Some(Self {
            lines: text
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
                .map(ScriptLine::new)
                .collect(),
            wait: WaitFrames::default(),
            until: None,
        })
    }

    fn insert_next(&mut self, lines: Vec<ScriptLine>) {
        for line in lines.into_iter().rev() {
            self.lines.push_front(line);
        }
    }
}

impl<Application: App> Runner<Application> {
    pub(crate) fn step_script(&mut self) -> Exit {
        let Some(script) = self.script.as_mut() else {
            return Exit::Stay;
        };
        if !script.wait.is_zero() {
            script.wait = script.wait.previous();
            return Exit::Stay;
        }
        if let Some(until) = script.until {
            if Instant::now() < until {
                return Exit::Stay;
            }
            script.until = None;
        }
        self.input.pointer.mods = self.mods;
        loop {
            let Some(line) = self
                .script
                .as_mut()
                .and_then(|queued| queued.lines.pop_front())
            else {
                return Exit::Stay;
            };
            match self.run_line(line) {
                Flow::Next => {}
                Flow::Yield => return Exit::Stay,
                Flow::Quit => return Exit::Quit,
            }
        }
    }

    fn insert_next(&mut self, lines: Vec<ScriptLine>) {
        if let Some(script) = self.script.as_mut() {
            script.insert_next(lines);
        }
    }

    fn run_line(&mut self, line: ScriptLine) -> Flow {
        let pointer = &mut self.input.pointer;
        match line.word(0).unwrap_or_default() {
            "wait" => {
                if let Some(script) = self.script.as_mut() {
                    script.wait = line.wait_frames();
                }
                Flow::Yield
            }
            "pause" => {
                if let Some(script) = self.script.as_mut() {
                    script.until = Some(Instant::now() + line.pause());
                }
                Flow::Yield
            }
            "mouse" => {
                pointer.mouse = line.point(1);
                Flow::Next
            }
            "down" => {
                pointer.down.insert(Button::Left);
                pointer.pressed.insert(Button::Left);
                pointer.clicks.set(
                    Button::Left,
                    if line.contains_word("twice") { 2 } else { 1 },
                );
                pointer.mods = line.mods_from(1);
                Flow::Yield
            }
            "up" => {
                pointer.down.remove(Button::Left);
                Flow::Yield
            }
            "click" | "dblclick" => {
                self.insert_next(line.click());
                Flow::Next
            }
            "click-id" | "hover-id" | "dblclick-id" => {
                self.aim(&line);
                Flow::Next
            }
            "drag" => {
                self.insert_next(line.drag());
                Flow::Next
            }
            "wheel" => {
                let vertical =
                    pointer.wheel.vertical.get() + Coordinate::of_integer(line.number(1)).get();
                pointer.wheel.vertical = Coordinate::new(vertical);
                pointer.mods = line.mods_from(2);
                Flow::Yield
            }
            "key" => {
                self.input.keys.push(Press {
                    key: line.key(),
                    mods: line.mods_from(2),
                });
                Flow::Yield
            }
            "text" => {
                self.input.typed.push_str(&line.rest(1));
                Flow::Yield
            }
            "quit" => Flow::Quit,
            _ => self.hand_to_app(line),
        }
    }

    fn aim(&mut self, line: &ScriptLine) {
        let Some(name) = line.word(1) else {
            return;
        };
        let located = line.element().and_then(|id| self.app.locate(id));
        match located {
            Some(at) => self.insert_next(vec![line.aimed(at)]),
            None => report(format_args!("script: no element '{name}' last frame")),
        }
    }

    fn hand_to_app(&mut self, line: ScriptLine) -> Flow {
        if line.as_str() == "dump" {
            self.statistics.dump(self.start);
        }
        if self.app.script(&line) == Outcome::Retry {
            self.insert_next(vec![line]);
            return Flow::Yield;
        }
        if line.as_str().starts_with("shot") {
            return Flow::Yield;
        }
        Flow::Next
    }
}
