use std::fmt;
use std::io::{self, Write};

pub(crate) fn report(message: fmt::Arguments<'_>) {
    let mut output = io::stderr().lock();
    drop(writeln!(output, "{message}"));
}
