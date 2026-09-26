use std::fmt;
use std::io;
use std::io::Write;

use cli::{Channel, Output};

pub(crate) fn text(channel: Channel, output: &Output) {
    write(channel, format_args!("{output}"));
}

pub(crate) fn line(channel: Channel, message: &impl fmt::Display) {
    write(channel, format_args!("{message}\n"));
}

fn write(channel: Channel, arguments: fmt::Arguments<'_>) {
    match channel {
        Channel::Stdout => io::stdout().lock().write_fmt(arguments).unwrap_or_default(),
        Channel::Stderr => io::stderr().lock().write_fmt(arguments).unwrap_or_default(),
    }
}
