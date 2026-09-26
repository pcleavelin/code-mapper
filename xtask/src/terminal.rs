use std::io::{Write, stderr, stdout};

use crate::text::Message;

pub(crate) fn say(message: &Message) {
    let mut out = stdout().lock();
    writeln!(out, "{message}").ok();
}

pub(crate) fn complain(message: &Message) {
    let mut out = stderr().lock();
    writeln!(out, "{message}").ok();
}
