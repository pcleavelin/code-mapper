mod convert;
mod exec;
mod failure;
mod output;
mod wire;

use domain::{Author, Changed, Index, Map, MapError, SymbolId};
use features::Feature;
use index::{ServerNotice, Servers, StartError};

use crate::convert::Request;
use crate::exec::Run;

pub use crate::convert::{Count, StepIndex, Under};
pub use crate::failure::{Candidate, Failure, StepPlace};
pub use crate::output::Output;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Argument(String);

impl Argument {
    pub fn new(text: &str) -> Self {
        Self(text.to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandLine(String);

impl CommandLine {
    pub fn new(line: &str) -> Self {
        Self(line.to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn arguments(&self) -> Vec<Argument> {
        wire::words(&self.0).into_iter().map(Argument).collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    Stdout,
    Stderr,
}

#[derive(Debug)]
pub struct ParseFailure(clap::Error);

impl ParseFailure {
    pub fn text(&self) -> Output {
        wire::render_error(&self.0)
    }

    pub fn channel(&self) -> Channel {
        if self.0.use_stderr() {
            Channel::Stderr
        } else {
            Channel::Stdout
        }
    }
}

#[derive(Debug)]
pub struct Invocation {
    feature: Feature,
    request: Request,
}

impl Invocation {
    pub fn parse(arguments: &[Argument]) -> Result<Self, ParseFailure> {
        wire::parse(arguments.iter().map(Argument::as_str))
            .map(|command| Self {
                feature: command.feature(),
                request: Request::from(command),
            })
            .map_err(ParseFailure)
    }

    pub fn feature(&self) -> Feature {
        self.feature
    }
}

pub fn help() -> Output {
    wire::help()
}

pub fn notice(notice: &ServerNotice) -> Output {
    wire::notice(notice)
}

pub fn start_error(error: &StartError) -> Output {
    wire::start_error(error)
}

pub fn symbol_label(index: &Index, symbol: SymbolId) -> Output {
    Output::of(wire::symbol_label(index, symbol))
}

pub fn map_failure(map: &Map, error: MapError) -> Failure {
    exec::map_failure(map, error)
}

pub fn exec(
    index: &mut Index,
    map: &mut Map,
    invocation: Invocation,
    author: Author,
    servers: Option<&mut Servers>,
    output: &mut Output,
) -> Result<Option<Changed>, Failure> {
    Run {
        index,
        map,
        author,
        servers,
        output,
    }
    .run(invocation.request)
}

#[cfg(test)]
mod tests;
