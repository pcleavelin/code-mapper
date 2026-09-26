mod spec;

pub use spec::{Chord, Element, Gesture, Key, Letter, Modifiers, Spec, Surface, Text, Trigger};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Feature {
    Files,
    Symbols,
    Show,
    Grep,
    Notes,
    Callers,
    Callees,
    Refs,
    Index,
    Tree,
    Roots,
    Paths,
    Path,
    PathNew,
    PathGroup,
    Groups,
    GroupRename,
    PathNote,
    StepNote,
    StepLink,
    StepUnlink,
    NoteEdit,
    PathRename,
    PathAdd,
    PathPin,
    PathMove,
    PathSwap,
    PathRm,
    Promote,
    Stale,
    Check,
    Repin,
    Uncovered,
    Coverage,
    Diff,
    Help,
    OpenPath,
    OpenGroup,
    SelectStep,
    WalkSteps,
    ToggleWholeSymbol,
    ToggleCode,
    ToggleFold,
    ExpandLink,
    OpenLinkedPath,
    HideAllCode,
    FoldAll,
    MoreContext,
    RemoveStep,
    RemovePath,
    ShowGraph,
    SwitchTab,
    GoBack,
    Save,
    PinSelection,
    NewPath,
    Search,
    RunCommand,
    FilterSymbols,
    BrowseFiles,
    FollowXref,
    GoToLine,
    SelectLines,
    JumpToDefinition,
    PeekDefinition,
    HoverInfo,
    ScrollCode,
    ExpandNode,
    MoveNode,
    NodeContext,
    NodeListing,
    AutoLayout,
    FitGraph,
    PanGraph,
    ZoomGraph,
    DockPanel,
    ResizePanel,
    OpenDiffRow,
}

impl Feature {
    pub const ALL: [Self; 78] = [
        Self::Files,
        Self::Symbols,
        Self::Show,
        Self::Grep,
        Self::Notes,
        Self::Callers,
        Self::Callees,
        Self::Refs,
        Self::Index,
        Self::Tree,
        Self::Roots,
        Self::Paths,
        Self::Path,
        Self::PathNew,
        Self::PathGroup,
        Self::Groups,
        Self::GroupRename,
        Self::PathNote,
        Self::StepNote,
        Self::StepLink,
        Self::StepUnlink,
        Self::NoteEdit,
        Self::PathRename,
        Self::PathAdd,
        Self::PathPin,
        Self::PathMove,
        Self::PathSwap,
        Self::PathRm,
        Self::Promote,
        Self::Stale,
        Self::Check,
        Self::Repin,
        Self::Uncovered,
        Self::Coverage,
        Self::Diff,
        Self::Help,
        Self::OpenPath,
        Self::OpenGroup,
        Self::SelectStep,
        Self::WalkSteps,
        Self::ToggleWholeSymbol,
        Self::ToggleCode,
        Self::ToggleFold,
        Self::ExpandLink,
        Self::OpenLinkedPath,
        Self::HideAllCode,
        Self::FoldAll,
        Self::MoreContext,
        Self::RemoveStep,
        Self::RemovePath,
        Self::ShowGraph,
        Self::SwitchTab,
        Self::GoBack,
        Self::Save,
        Self::PinSelection,
        Self::NewPath,
        Self::Search,
        Self::RunCommand,
        Self::FilterSymbols,
        Self::BrowseFiles,
        Self::FollowXref,
        Self::GoToLine,
        Self::SelectLines,
        Self::JumpToDefinition,
        Self::PeekDefinition,
        Self::HoverInfo,
        Self::ScrollCode,
        Self::ExpandNode,
        Self::MoveNode,
        Self::NodeContext,
        Self::NodeListing,
        Self::AutoLayout,
        Self::FitGraph,
        Self::PanGraph,
        Self::ZoomGraph,
        Self::DockPanel,
        Self::ResizePanel,
        Self::OpenDiffRow,
    ];

    #[expect(
        clippy::too_many_lines,
        reason = "the registry is one table, a match arm per feature"
    )]
    pub fn spec(self) -> Spec {
        match self {
            Self::Files => Spec::new(
                Text::new("files"),
                Text::new("[filter]                         list indexed files (substring filter)"),
                Surface::Command,
                &const { [Trigger::Command(Text::new("files"))] },
            ),
            Self::Symbols => Spec::new(
                Text::new("symbols"),
                Text::new(
                    "[filter]                         list symbols: file:start-end kind name (calls/callers)",
                ),
                Surface::Command,
                &const { [Trigger::Command(Text::new("symbols"))] },
            ),
            Self::Show => Spec::new(
                Text::new("show"),
                Text::new(
                    "<file> [start] [end]             print numbered lines (1-based, inclusive)",
                ),
                Surface::Command,
                &const { [Trigger::Command(Text::new("show"))] },
            ),
            Self::Grep => Spec::new(
                Text::new("grep"),
                Text::new("<regex>                          file:line: text"),
                Surface::Command,
                &const { [Trigger::Command(Text::new("grep"))] },
            ),
            Self::Notes => Spec::new(
                Text::new("notes"),
                Text::new("<regex>                          search path notes and step notes"),
                Surface::Command,
                &const { [Trigger::Command(Text::new("notes"))] },
            ),
            Self::Callers => Spec::new(
                Text::new("callers"),
                Text::new("<symbol>                         who calls it (xrefs to)"),
                Surface::Command,
                &const { [Trigger::Command(Text::new("callers"))] },
            ),
            Self::Callees => Spec::new(
                Text::new("callees"),
                Text::new("<symbol>                         what it calls (xrefs from)"),
                Surface::Command,
                &const { [Trigger::Command(Text::new("callees"))] },
            ),
            Self::Refs => Spec::new(
                Text::new("refs"),
                Text::new(
                    "<symbol>                         every reference to it, file:line: text (asks the language's server)",
                ),
                Surface::Command,
                &const { [Trigger::Command(Text::new("refs"))] },
            ),
            Self::Index => Spec::new(
                Text::new("index"),
                Text::new(
                    "[filter]                         ask the language servers now for every file whose path contains filter (commands otherwise ask for the files they touch)",
                ),
                Surface::Command,
                &const { [Trigger::Command(Text::new("index"))] },
            ),
            Self::Tree => Spec::new(
                Text::new("tree"),
                Text::new(
                    "<symbol> [depth]                 call tree from a symbol (default depth 4)",
                ),
                Surface::Command,
                &const { [Trigger::Command(Text::new("tree"))] },
            ),
            Self::Roots => Spec::new(
                Text::new("roots"),
                Text::new(
                    "[n]                              entry points: symbols nobody calls (default 30)",
                ),
                Surface::Command,
                &const { [Trigger::Command(Text::new("roots"))] },
            ),
            Self::Paths => Spec::new(
                Text::new("paths"),
                Text::new(
                    "[name]                           the map: every path (or one) as a tree of steps (! = stale, (ai) = AI-authored, → = link)",
                ),
                Surface::Command,
                &const { [Trigger::Command(Text::new("paths"))] },
            ),
            Self::Path => Spec::new(
                Text::new("path"),
                Text::new(
                    "<name> [--expand]                print a path's note and every step's code, tree order; --expand prints linked paths inline",
                ),
                Surface::Command,
                &const { [Trigger::Command(Text::new("path"))] },
            ),
            Self::PathNew => Spec::new(
                Text::new("path-new"),
                Text::new(
                    "<name> <kind> [note] [--group g] create a path; kind = flow | layer | type (no-op if it exists)",
                ),
                Surface::Command,
                &const { [Trigger::Command(Text::new("path-new"))] },
            ),
            Self::PathGroup => Spec::new(
                Text::new("path-group"),
                Text::new(
                    "<name> <group>                   put a path in a group; / nests groups (flows/http), \"\" = top level",
                ),
                Surface::Command,
                &const { [Trigger::Command(Text::new("path-group"))] },
            ),
            Self::Groups => Spec::new(
                Text::new("groups"),
                Text::new("every group with its paths, nested"),
                Surface::Command,
                &const { [Trigger::Command(Text::new("groups"))] },
            ),
            Self::GroupRename => Spec::new(
                Text::new("group-rename"),
                Text::new(
                    "<old> <new>                      rename a group and every group inside it",
                ),
                Surface::Command,
                &const { [Trigger::Command(Text::new("group-rename"))] },
            ),
            Self::PathNote => Spec::new(
                Text::new("path-note"),
                Text::new("<name> <note>                    set a path's note"),
                Surface::Command,
                &const { [Trigger::Command(Text::new("path-note"))] },
            ),
            Self::StepNote => Spec::new(
                Text::new("step-note"),
                Text::new(
                    "<name> <index> <note>            set a note on one step (index as shown by `paths`)",
                ),
                Surface::Command,
                &const { [Trigger::Command(Text::new("step-note"))] },
            ),
            Self::StepLink => Spec::new(
                Text::new("step-link"),
                Text::new(
                    "<name> <index> <target>          link a step to the path that documents what its lines call",
                ),
                Surface::Command,
                &const { [Trigger::Command(Text::new("step-link"))] },
            ),
            Self::StepUnlink => Spec::new(
                Text::new("step-unlink"),
                Text::new("<name> <index>                   remove a step's link"),
                Surface::Command,
                &const { [Trigger::Command(Text::new("step-unlink"))] },
            ),
            Self::NoteEdit => Spec::new(
                Text::new("note-edit"),
                Text::new(
                    "<name> <index> <old> <new>       replace the first `old` in a note with `new` (index -1 = the path note)",
                ),
                Surface::Command,
                &const { [Trigger::Command(Text::new("note-edit"))] },
            ),
            Self::PathRename => Spec::new(
                Text::new("path-rename"),
                Text::new("<name> <new>                     rename a path"),
                Surface::Command,
                &const { [Trigger::Command(Text::new("path-rename"))] },
            ),
            Self::PathAdd => Spec::new(
                Text::new("path-add"),
                Text::new(
                    "<name> <sym|file start end> [under]  add a step under step `under` (default: the last step; -1 = root)",
                ),
                Surface::Command,
                &const { [Trigger::Command(Text::new("path-add"))] },
            ),
            Self::PathPin => Spec::new(
                Text::new("path-pin"),
                Text::new(
                    "<name> <index> <file> <start> <end>  re-anchor a step; its note and place in the tree stay",
                ),
                Surface::Command,
                &const { [Trigger::Command(Text::new("path-pin"))] },
            ),
            Self::PathMove => Spec::new(
                Text::new("path-move"),
                Text::new(
                    "<name> <index> <under>          move a step (with its subtree) under step `under` (-1 = root)",
                ),
                Surface::Command,
                &const { [Trigger::Command(Text::new("path-move"))] },
            ),
            Self::PathSwap => Spec::new(
                Text::new("path-swap"),
                Text::new(
                    "<name> <a> <b>                  swap two steps' places in the list, which orders siblings when the code does not",
                ),
                Surface::Command,
                &const { [Trigger::Command(Text::new("path-swap"))] },
            ),
            Self::PathRm => Spec::new(
                Text::new("path-rm"),
                Text::new(
                    "<name> [index]                   delete a step (its children move up) or the whole path",
                ),
                Surface::Command,
                &const { [Trigger::Command(Text::new("path-rm"))] },
            ),
            Self::Promote => Spec::new(
                Text::new("promote"),
                Text::new(
                    "<symbol> [depth] [name]          create a path shaped like a symbol's call tree (default depth 1, named after the symbol)",
                ),
                Surface::Command,
                &const { [Trigger::Command(Text::new("promote"))] },
            ),
            Self::Stale => Spec::new(
                Text::new("stale"),
                Text::new(
                    "every step whose text no longer matches, or whose file or symbol is gone",
                ),
                Surface::Command,
                &const { [Trigger::Command(Text::new("stale"))] },
            ),
            Self::Check => Spec::new(
                Text::new("check"),
                Text::new("exit non-zero if any step is stale"),
                Surface::Command,
                &const { [Trigger::Command(Text::new("check"))] },
            ),
            Self::Repin => Spec::new(
                Text::new("repin"),
                Text::new(
                    "[rev]                            re-pin every stale step by following its text from revision `rev` (default: the parent, @- in jj, HEAD in git); prints each change so its note gets reread",
                ),
                Surface::Command,
                &const { [Trigger::Command(Text::new("repin"))] },
            ),
            Self::Uncovered => Spec::new(
                Text::new("uncovered"),
                Text::new("[filter]                         symbols in no path, largest first"),
                Surface::Command,
                &const { [Trigger::Command(Text::new("uncovered"))] },
            ),
            Self::Coverage => Spec::new(
                Text::new("coverage"),
                Text::new("covered/total symbols per file"),
                Surface::Command,
                &const { [Trigger::Command(Text::new("coverage"))] },
            ),
            Self::Diff => Spec::new(
                Text::new("diff"),
                Text::new("the map against the parent revision's (@- in jj, HEAD in git)"),
                Surface::Command,
                &const { [Trigger::Command(Text::new("diff"))] },
            ),
            Self::Help => Spec::new(
                Text::new("help"),
                Text::new("every command with its arguments"),
                Surface::Command,
                &const { [Trigger::Command(Text::new("help"))] },
            ),
            Self::OpenPath => Spec::new(
                Text::new("open-path"),
                Text::new("read a path as a document: click it in the Paths list"),
                Surface::Window,
                &const { [Trigger::Click(Element::new("paths"))] },
            ),
            Self::OpenGroup => Spec::new(
                Text::new("open-group"),
                Text::new("open or close a group of paths in the Paths list"),
                Surface::Window,
                &const { [Trigger::Click(Element::new("group"))] },
            ),
            Self::SelectStep => Spec::new(
                Text::new("select-step"),
                Text::new("select a step from its header, the outline or the breadcrumb"),
                Surface::Window,
                &const {
                    [
                        Trigger::Click(Element::new("step")),
                        Trigger::Click(Element::new("outline")),
                        Trigger::Click(Element::new("crumb")),
                    ]
                },
            ),
            Self::WalkSteps => Spec::new(
                Text::new("walk-steps"),
                Text::new("move to the next or previous step with the arrow keys"),
                Surface::Window,
                &const {
                    [
                        Trigger::Key(Chord::plain(Key::Down)),
                        Trigger::Key(Chord::plain(Key::Up)),
                    ]
                },
            ),
            Self::ToggleWholeSymbol => Spec::new(
                Text::new("toggle-whole-symbol"),
                Text::new(
                    "show the whole enclosing symbol around a step's slice, or the slice alone",
                ),
                Surface::Window,
                &const { [Trigger::Click(Element::new("whole"))] },
            ),
            Self::ToggleCode => Spec::new(
                Text::new("toggle-code"),
                Text::new("hide or show a step's code"),
                Surface::Window,
                &const { [Trigger::Click(Element::new("hide"))] },
            ),
            Self::ToggleFold => Spec::new(
                Text::new("toggle-fold"),
                Text::new("fold or unfold the steps under a step"),
                Surface::Window,
                &const { [Trigger::Click(Element::new("fold"))] },
            ),
            Self::ExpandLink => Spec::new(
                Text::new("expand-link"),
                Text::new("show the path a step links to inline under it"),
                Surface::Window,
                &const { [Trigger::Click(Element::new("expand"))] },
            ),
            Self::OpenLinkedPath => Spec::new(
                Text::new("open-linked-path"),
                Text::new("open the path a step links to, or a step that links here"),
                Surface::Window,
                &const {
                    [
                        Trigger::Click(Element::new("link")),
                        Trigger::Click(Element::new("from")),
                    ]
                },
            ),
            Self::HideAllCode => Spec::new(
                Text::new("hide-all-code"),
                Text::new("hide every step's code, or show all again"),
                Surface::Window,
                &const {
                    [
                        Trigger::Click(Element::new("doc-collapse")),
                        Trigger::Click(Element::new("doc-expand")),
                    ]
                },
            ),
            Self::FoldAll => Spec::new(
                Text::new("fold-all"),
                Text::new("fold every step with children, or unfold all"),
                Surface::Window,
                &const {
                    [
                        Trigger::Click(Element::new("doc-fold")),
                        Trigger::Click(Element::new("doc-unfold")),
                    ]
                },
            ),
            Self::MoreContext => Spec::new(
                Text::new("more-context"),
                Text::new(
                    "show ten more lines of the file above or below a step's slice, or back to the slice",
                ),
                Surface::Window,
                &const {
                    [
                        Trigger::Click(Element::new("ctx-a")),
                        Trigger::Click(Element::new("ctx-b")),
                        Trigger::Click(Element::new("ctx0")),
                    ]
                },
            ),
            Self::RemoveStep => Spec::new(
                Text::new("remove-step"),
                Text::new("delete a step; its children move up to its parent"),
                Surface::Window,
                &const { [Trigger::Click(Element::new("del"))] },
            ),
            Self::RemovePath => Spec::new(
                Text::new("remove-path"),
                Text::new("delete the path being read"),
                Surface::Window,
                &const { [Trigger::Click(Element::new("doc-delete"))] },
            ),
            Self::ShowGraph => Spec::new(
                Text::new("show-graph"),
                Text::new("draw the path being read as a graph"),
                Surface::Window,
                &const { [Trigger::Click(Element::new("doc-graph"))] },
            ),
            Self::SwitchTab => Spec::new(
                Text::new("switch-tab"),
                Text::new("switch the centre or left tab"),
                Surface::Window,
                &const {
                    [
                        Trigger::Click(Element::new("tab")),
                        Trigger::Click(Element::new("left")),
                    ]
                },
            ),
            Self::GoBack => Spec::new(
                Text::new("go-back"),
                Text::new("go back to the previous place, or forward again"),
                Surface::Window,
                &const {
                    [
                        Trigger::Click(Element::new("back")),
                        Trigger::Click(Element::new("forward")),
                        Trigger::Key(Chord::alt(Key::Left)),
                        Trigger::Key(Chord::alt(Key::Right)),
                        Trigger::Key(Chord::control(Key::Left)),
                        Trigger::Key(Chord::control(Key::Right)),
                        Trigger::Gesture(Gesture::BackButton, Element::new("window")),
                        Trigger::Gesture(Gesture::ForwardButton, Element::new("window")),
                    ]
                },
            ),
            Self::Save => Spec::new(
                Text::new("save"),
                Text::new("write the map's changed paths to disk"),
                Surface::Window,
                &const {
                    [
                        Trigger::Click(Element::new("save")),
                        Trigger::Key(Chord::control(Key::Letter(Letter::new('s')))),
                    ]
                },
            ),
            Self::PinSelection => Spec::new(
                Text::new("pin-selection"),
                Text::new("pin the lines selected in the listing as a step of the path being read"),
                Surface::Window,
                &const { [Trigger::Click(Element::new("pin"))] },
            ),
            Self::NewPath => Spec::new(
                Text::new("new-path"),
                Text::new("create a flow path named in the new path field"),
                Surface::Window,
                &const { [Trigger::Type(Element::new("field@new-path"))] },
            ),
            Self::Search => Spec::new(
                Text::new("search"),
                Text::new("grep every file for a regex; the hits open in the Results tab"),
                Surface::Window,
                &const {
                    [
                        Trigger::Type(Element::new("field@search")),
                        Trigger::Click(Element::new("hit")),
                    ]
                },
            ),
            Self::RunCommand => Spec::new(
                Text::new("run-command"),
                Text::new("run a CLI command in the output panel"),
                Surface::Window,
                &const { [Trigger::Type(Element::new("cmd"))] },
            ),
            Self::FilterSymbols => Spec::new(
                Text::new("filter-symbols"),
                Text::new("filter the Symbols tab and focus a symbol from it"),
                Surface::Window,
                &const {
                    [
                        Trigger::Type(Element::new("field@symbols")),
                        Trigger::Click(Element::new("sym")),
                    ]
                },
            ),
            Self::BrowseFiles => Spec::new(
                Text::new("browse-files"),
                Text::new("open a directory or a file in the Files tab"),
                Surface::Window,
                &const {
                    [
                        Trigger::Click(Element::new("dir")),
                        Trigger::Click(Element::new("file")),
                    ]
                },
            ),
            Self::FollowXref => Spec::new(
                Text::new("follow-xref"),
                Text::new("go to a caller, callee or reference listed for the selected symbol"),
                Surface::Window,
                &const {
                    [
                        Trigger::Click(Element::new("xto")),
                        Trigger::Click(Element::new("xfrom")),
                        Trigger::Click(Element::new("xref")),
                    ]
                },
            ),
            Self::GoToLine => Spec::new(
                Text::new("go-to-line"),
                Text::new("jump the listing to a line number"),
                Surface::Window,
                &const { [Trigger::Type(Element::new("field@goto-line"))] },
            ),
            Self::SelectLines => Spec::new(
                Text::new("select-lines"),
                Text::new("select a line, or extend the selection, in the listing"),
                Surface::Window,
                &const {
                    [
                        Trigger::Click(Element::new("lines")),
                        Trigger::Gesture(Gesture::ShiftClick, Element::new("lines")),
                    ]
                },
            ),
            Self::JumpToDefinition => Spec::new(
                Text::new("jump-to-definition"),
                Text::new("go to the definition of the identifier under the pointer"),
                Surface::Window,
                &const {
                    [
                        Trigger::Gesture(Gesture::ControlClick, Element::new("code")),
                        Trigger::Gesture(Gesture::DoubleClick, Element::new("code")),
                    ]
                },
            ),
            Self::PeekDefinition => Spec::new(
                Text::new("peek-definition"),
                Text::new("pin the definition of the identifier under the pointer above the xrefs"),
                Surface::Window,
                &const {
                    [
                        Trigger::Gesture(Gesture::AltClick, Element::new("code")),
                        Trigger::Click(Element::new("peek-go")),
                        Trigger::Click(Element::new("peek-x")),
                    ]
                },
            ),
            Self::HoverInfo => Spec::new(
                Text::new("hover-info"),
                Text::new(
                    "show what the language's server knows about the identifier under the pointer",
                ),
                Surface::Window,
                &const { [Trigger::Gesture(Gesture::Hover, Element::new("code"))] },
            ),
            Self::ScrollCode => Spec::new(
                Text::new("scroll-code"),
                Text::new("scroll a code block sideways"),
                Surface::Window,
                &const { [Trigger::Gesture(Gesture::ShiftWheel, Element::new("code"))] },
            ),
            Self::ExpandNode => Spec::new(
                Text::new("expand-node"),
                Text::new("reveal a graph node's callers or callees, or hide what it revealed"),
                Surface::Window,
                &const { [Trigger::Click(Element::new("node-button"))] },
            ),
            Self::MoveNode => Spec::new(
                Text::new("move-node"),
                Text::new("drag a graph node by its title"),
                Surface::Window,
                &const { [Trigger::Gesture(Gesture::Drag, Element::new("node"))] },
            ),
            Self::NodeContext => Spec::new(
                Text::new("node-context"),
                Text::new("show more of a graph node's file around it, or cut it to its own lines"),
                Surface::Window,
                &const { [Trigger::Click(Element::new("node-context"))] },
            ),
            Self::NodeListing => Spec::new(
                Text::new("node-listing"),
                Text::new("open a graph node's file in the listing"),
                Surface::Window,
                &const { [Trigger::Click(Element::new("node-listing"))] },
            ),
            Self::AutoLayout => Spec::new(
                Text::new("auto-layout"),
                Text::new("lay the graph out again, dropping dragged positions"),
                Surface::Window,
                &const { [Trigger::Click(Element::new("graph-auto"))] },
            ),
            Self::FitGraph => Spec::new(
                Text::new("fit-graph"),
                Text::new("zoom the graph to show all of it, or back to 1:1"),
                Surface::Window,
                &const {
                    [
                        Trigger::Click(Element::new("graph-fit")),
                        Trigger::Click(Element::new("graph-1to1")),
                    ]
                },
            ),
            Self::PanGraph => Spec::new(
                Text::new("pan-graph"),
                Text::new("pan the graph by dragging its background or with the wheel"),
                Surface::Window,
                &const {
                    [
                        Trigger::Gesture(Gesture::Drag, Element::new("graph-canvas")),
                        Trigger::Gesture(Gesture::Wheel, Element::new("graph-canvas")),
                    ]
                },
            ),
            Self::ZoomGraph => Spec::new(
                Text::new("zoom-graph"),
                Text::new("zoom the graph around the pointer"),
                Surface::Window,
                &const {
                    [Trigger::Gesture(
                        Gesture::ControlWheel,
                        Element::new("graph-canvas"),
                    )]
                },
            ),
            Self::DockPanel => Spec::new(
                Text::new("dock-panel"),
                Text::new("drag a panel by its header to another edge of the window"),
                Surface::Window,
                &const { [Trigger::Gesture(Gesture::Drag, Element::new("grip"))] },
            ),
            Self::ResizePanel => Spec::new(
                Text::new("resize-panel"),
                Text::new("drag a panel's splitter to resize it"),
                Surface::Window,
                &const { [Trigger::Gesture(Gesture::Drag, Element::new("split"))] },
            ),
            Self::OpenDiffRow => Spec::new(
                Text::new("open-diff-row"),
                Text::new("open a path from the map diff, or refresh the diff"),
                Surface::Window,
                &const {
                    [
                        Trigger::Click(Element::new("diffrow")),
                        Trigger::Click(Element::new("diff-refresh")),
                    ]
                },
            ),
        }
    }
}

#[cfg(test)]
mod tests;
