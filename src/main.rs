mod cli;
mod codec;
mod gfx;
mod gui;
mod index;
mod lsp;
mod map;
mod ui;
mod vcs;
mod window;

use map::{Author, Map};
use std::path::{Path, PathBuf};

fn cli_main(root: &Path, args: &[String]) -> i32 {
    let cmd = match cli::parse(args) {
        Ok(cmd) => cmd,
        Err(e) => {
            let _ = e.print();
            return e.exit_code();
        }
    };
    let mut idx = index::build(root);
    let map_path = root.join(map::MAP_DIR);
    // a map that does not read stops every command, so no save can write over what is there
    let mut map = match Map::load(&map_path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    map.resolve_all(&idx);
    let mut out = String::new();
    // A closed pipe (`| head`) is not an error worth a panic.
    let emit = |s: &str| {
        let _ = std::io::Write::write_all(&mut std::io::stdout(), s.as_bytes());
    };
    let mut servers = index::Servers::new(root, |m| eprintln!("{m}"));
    let done = cli::exec(&mut idx, &mut map, cmd, Author::Ai, Some(&mut servers), &mut out);
    drop(servers);
    match done {
        Ok(dirty) => {
            // the map is saved before the command's output is shown, so a line that says a
            // step was added is never printed for a change that did not reach the disk
            if dirty {
                if let Err(e) = map.save(&map_path) {
                    emit(&out);
                    eprintln!("save failed: {e}");
                    return 1;
                }
            }
            emit(&out);
            0
        }
        Err(e) => {
            emit(&out);
            eprintln!("{e}");
            2
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "help" || a == "--help" || a == "-h") {
        print!("{}", cli::help());
        return;
    }
    let root = args.first().map(PathBuf::from).unwrap_or_else(|| std::env::current_dir().expect("cwd"));
    if args.len() > 1 {
        std::process::exit(cli_main(&root, &args[1..]));
    }
    let title = format!("codemap - {}", root.display());
    window::run(&title, gui::App::new(&root));
}
