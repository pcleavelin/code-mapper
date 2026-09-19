mod cli;
mod gfx;
mod graph;
mod gui;
mod index;
mod lsp;
mod map;
mod ui;

use map::{Author, Map};
use std::path::{Path, PathBuf};

fn cli_main(root: &Path, args: &[String]) -> i32 {
    let cmd = match cli::parse(args) {
        Ok(cmd) => cmd,
        Err(e) if e.use_stderr() => {
            eprint!("{e}");
            return 2;
        }
        Err(e) => {
            print!("{e}");
            return 0;
        }
    };
    let mut idx = index::build(root);
    idx.run_backends(|m| eprintln!("{m}"));
    let map_path = root.join(".codemap");
    let map = Map::load(&map_path);
    if map.is_none() && map_path.exists() {
        eprintln!("{}: unreadable or an old format, starting from an empty map", map_path.display());
    }
    let mut map = map.unwrap_or_default();
    map.resolve_all(&idx);
    let mut out = String::new();
    // A closed pipe (`| head`) is not an error worth a panic.
    let emit = |s: &str| {
        let _ = std::io::Write::write_all(&mut std::io::stdout(), s.as_bytes());
    };
    match cli::exec(&idx, &mut map, cmd, Author::Ai, &mut out) {
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
    gfx::run(&title, gui::App::new(&root));
}
