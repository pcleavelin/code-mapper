//! The CLI, the agent's interface, pinned command by command against golden transcripts.
//! `CODEMAP_BLESS=1 cargo test --test cli` rewrites them.

mod common;

fn scenario(name: &str) {
    let (_, run) = common::cli::SCENARIOS
        .iter()
        .find(|(n, _)| *n == name)
        .unwrap();
    common::golden(name, &run(&common::bin()));
}

#[test]
fn read() {
    scenario("cli-read");
}

#[test]
fn edit() {
    scenario("cli-edit");
}

#[test]
fn stale() {
    scenario("cli-stale");
}

#[test]
fn vcs() {
    scenario("cli-vcs");
}

#[test]
fn git() {
    scenario("cli-git");
}

#[test]
fn merge() {
    scenario("cli-merge");
}
