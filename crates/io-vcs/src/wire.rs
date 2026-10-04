use std::iter;

pub(crate) struct CommandLine {
    pub(crate) program: &'static str,
    pub(crate) arguments: Vec<String>,
}

pub(crate) const JJ: &str = "jj";
pub(crate) const GIT: &str = "git";
const MAP_EXTENSION: &str = ".cmap";

pub(crate) const JJ_PARENT: &str = "@-";
pub(crate) const GIT_PARENT: &str = "HEAD";
pub(crate) const JJ_MARKER: &str = ".jj";
pub(crate) const GIT_MARKER: &str = ".git";
pub(crate) const JJ_REPO: &str = "repo";
pub(crate) const GIT_DIR_PREFIX: &str = "gitdir:";
pub(crate) const GIT_COMMON_DIR: &str = "commondir";

pub(crate) fn jj_show_file(revision: &str, path: &str) -> CommandLine {
    CommandLine {
        program: JJ,
        arguments: vec![
            "file".to_owned(),
            "show".to_owned(),
            "-r".to_owned(),
            revision.to_owned(),
            path.to_owned(),
        ],
    }
}

pub(crate) fn jj_show_map_files(revision: &str, directory: &str) -> CommandLine {
    jj_show_file(revision, &format!("glob:{directory}/*{MAP_EXTENSION}"))
}

pub(crate) fn git_show_files(revision: &str, paths: &[String]) -> CommandLine {
    CommandLine {
        program: GIT,
        arguments: iter::once("show".to_owned())
            .chain(paths.iter().map(|path| format!("{revision}:./{path}")))
            .collect(),
    }
}

pub(crate) fn git_list_tree(revision: &str, directory: &str) -> CommandLine {
    CommandLine {
        program: GIT,
        arguments: vec![
            "ls-tree".to_owned(),
            "--name-only".to_owned(),
            revision.to_owned(),
            "--".to_owned(),
            format!("{directory}/"),
        ],
    }
}

pub(crate) struct TreeListing {
    pub(crate) map_files: Vec<String>,
}

impl TreeListing {
    pub(crate) fn parse(stdout: &str) -> Self {
        let mut map_files: Vec<String> = stdout
            .lines()
            .filter(|name| name.ends_with(MAP_EXTENSION))
            .map(str::to_owned)
            .collect();
        map_files.sort();
        Self { map_files }
    }
}
