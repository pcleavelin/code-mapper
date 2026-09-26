use std::fmt;
use std::fmt::Write;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Output(String);

impl Output {
    pub fn new() -> Self {
        Self(String::new())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn extend(&mut self, other: &Self) {
        self.0.push_str(&other.0);
    }

    pub(crate) fn of(text: String) -> Self {
        Self(text)
    }

    pub(crate) fn line(&mut self, arguments: fmt::Arguments<'_>) {
        self.0.write_fmt(arguments).unwrap_or_default();
        self.0.push('\n');
    }
}

impl fmt::Display for Output {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

macro_rules! write_line {
    ($output:expr, $($format:tt)*) => {
        $output.line(format_args!($($format)*))
    };
}

pub(crate) use write_line;
