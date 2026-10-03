mod spec;

use strum::VariantArray;

pub use spec::{Chord, Element, Gesture, Key, Letter, Modifiers, Spec, Surface, Text, Trigger};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, VariantArray)]
pub enum Feature {
    Files,
    Symbols,
    Show,
    Search,
    Notes,
    Callers,
    Callees,
    Refs,
    Index,
    Tree,
    Roots,
    Tours,
    Tour,
    TourNew,
    TourGroup,
    Groups,
    GroupRename,
    TourNote,
    StepNote,
    StepLink,
    StepUnlink,
    NoteEdit,
    TourRename,
    TourAdd,
    TourPin,
    TourMove,
    TourSwap,
    TourRm,
    Promote,
    Stale,
    Check,
    Repin,
    Uncovered,
    Coverage,
    Diff,
    Help,
    OpenTour,
    OpenGroup,
    SelectStep,
    WalkSteps,
    ToggleWholeSymbol,
    ToggleCode,
    ToggleCollapse,
    InlineLink,
    OpenLinkedTour,
    HideAllCode,
    CollapseAll,
    MoreContext,
    RemoveStep,
    RemoveTour,
    ShowGraph,
    SwitchTab,
    GoBack,
    Save,
    AddStep,
    ChooseTarget,
    MoveStep,
    PromoteSymbol,
    SearchFiles,
    RunCommand,
    FilterSymbols,
    FilterTours,
    BrowseFiles,
    FollowXref,
    GoToLine,
    SelectLines,
    JumpToDefinition,
    PeekDefinition,
    HoverInfo,
    ScrollCode,
    RevealNode,
    MoveNode,
    NodeContext,
    NodeSource,
    ShowReferences,
    AutoLayout,
    FitGraph,
    PanGraph,
    ZoomGraph,
    WalkGraph,
    TurnGraph,
    MoveView,
    ResizePanel,
    SplitPanel,
    ClosePanel,
    CloseTab,
    PickView,
    OpenDiffRow,
    CommandPalette,
    Welcome,
    BuildTour,
    EditTour,
    EditText,
    Settings,
    ElementTip,
}

impl Feature {
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
            Self::Search => Spec::new(
                Text::new("search"),
                Text::new("<regex>                          file:line: text"),
                Surface::Command,
                &const { [Trigger::Command(Text::new("search"))] },
            ),
            Self::Notes => Spec::new(
                Text::new("notes"),
                Text::new("<regex>                          search tour notes and step notes"),
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
            Self::Tours => Spec::new(
                Text::new("tours"),
                Text::new(
                    "[name]                           the map: every tour (or one) as a tree of steps (! = stale, (ai) = AI-authored, → = link)",
                ),
                Surface::Command,
                &const { [Trigger::Command(Text::new("tours"))] },
            ),
            Self::Tour => Spec::new(
                Text::new("tour"),
                Text::new(
                    "<name> [--inline]                print a tour's note and every step's code, tree order; --inline prints linked tours inline",
                ),
                Surface::Command,
                &const { [Trigger::Command(Text::new("tour"))] },
            ),
            Self::TourNew => Spec::new(
                Text::new("tour-new"),
                Text::new(
                    "<name> <kind> [note] [--group g] create a tour; kind = flow | layer | data (no-op if it exists)",
                ),
                Surface::Command,
                &const { [Trigger::Command(Text::new("tour-new"))] },
            ),
            Self::TourGroup => Spec::new(
                Text::new("tour-group"),
                Text::new(
                    "<name> <group>                   put a tour in a group; / nests groups (flows/http), \"\" = top level",
                ),
                Surface::Command,
                &const { [Trigger::Command(Text::new("tour-group"))] },
            ),
            Self::Groups => Spec::new(
                Text::new("groups"),
                Text::new("every group with its tours, nested"),
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
            Self::TourNote => Spec::new(
                Text::new("tour-note"),
                Text::new("<name> <note>                    set a tour's note"),
                Surface::Command,
                &const { [Trigger::Command(Text::new("tour-note"))] },
            ),
            Self::StepNote => Spec::new(
                Text::new("step-note"),
                Text::new(
                    "<name> <index> <note>            set a note on one step (index as shown by `tours`)",
                ),
                Surface::Command,
                &const { [Trigger::Command(Text::new("step-note"))] },
            ),
            Self::StepLink => Spec::new(
                Text::new("step-link"),
                Text::new(
                    "<name> <index> <target>          link a step to the tour that documents what its lines call",
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
                    "<name> <index> <old> <new>       replace the first `old` in a note with `new` (index -1 = the tour note)",
                ),
                Surface::Command,
                &const { [Trigger::Command(Text::new("note-edit"))] },
            ),
            Self::TourRename => Spec::new(
                Text::new("tour-rename"),
                Text::new("<name> <new>                     rename a tour"),
                Surface::Command,
                &const { [Trigger::Command(Text::new("tour-rename"))] },
            ),
            Self::TourAdd => Spec::new(
                Text::new("tour-add"),
                Text::new(
                    "<name> <sym|file start end> [under]  add a step under step `under` (default: the last step; -1 = root)",
                ),
                Surface::Command,
                &const { [Trigger::Command(Text::new("tour-add"))] },
            ),
            Self::TourPin => Spec::new(
                Text::new("tour-pin"),
                Text::new(
                    "<name> <index> <file> <start> <end>  re-anchor a step; its note and place in the tree stay",
                ),
                Surface::Command,
                &const { [Trigger::Command(Text::new("tour-pin"))] },
            ),
            Self::TourMove => Spec::new(
                Text::new("tour-move"),
                Text::new(
                    "<name> <index> <under>          move a step (with its subtree) under step `under` (-1 = root)",
                ),
                Surface::Command,
                &const { [Trigger::Command(Text::new("tour-move"))] },
            ),
            Self::TourSwap => Spec::new(
                Text::new("tour-swap"),
                Text::new(
                    "<name> <a> <b>                  swap two steps' places in the list, which orders siblings when the code does not",
                ),
                Surface::Command,
                &const { [Trigger::Command(Text::new("tour-swap"))] },
            ),
            Self::TourRm => Spec::new(
                Text::new("tour-rm"),
                Text::new(
                    "<name> [index]                   delete a step (its children move up) or the whole tour",
                ),
                Surface::Command,
                &const { [Trigger::Command(Text::new("tour-rm"))] },
            ),
            Self::Promote => Spec::new(
                Text::new("promote"),
                Text::new(
                    "<symbol> [depth] [name] [--all]  create a tour shaped like a symbol's call tree (default depth 2, named after the symbol): tests, accessors and trivial bodies are left out, shared, other-package and already-mapped callees are kept as leaves, and each rule's symbols and the step-link lines for mapped leaves are printed; --all keeps the whole tree",
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
                Text::new("[filter]                         symbols in no tour, largest first"),
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
                Text::new("Print this message or the help of the given subcommand(s)"),
                Surface::Command,
                &const { [Trigger::Command(Text::new("help"))] },
            ),
            Self::OpenTour => Spec::new(
                Text::new("open-tour"),
                Text::new("read a tour as a document: click it in the Tours list"),
                Surface::Window,
                &const { [Trigger::Click(Element::new("tours"))] },
            ),
            Self::OpenGroup => Spec::new(
                Text::new("open-group"),
                Text::new("open or close a group of tours in the Tours list"),
                Surface::Window,
                &const { [Trigger::Click(Element::new("group"))] },
            ),
            Self::SelectStep => Spec::new(
                Text::new("select-step"),
                Text::new(
                    "select a step from its header, the steps list, the breadcrumb or a step the tours filter found",
                ),
                Surface::Window,
                &const {
                    [
                        Trigger::Click(Element::new("step")),
                        Trigger::Click(Element::new("steps")),
                        Trigger::Click(Element::new("crumb")),
                        Trigger::Click(Element::new("found")),
                    ]
                },
            ),
            Self::WalkSteps => Spec::new(
                Text::new("walk-steps"),
                Text::new(
                    "move to the next or previous step with the up and down arrow keys; after a click in the graph, to the node above or below",
                ),
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
            Self::ToggleCollapse => Spec::new(
                Text::new("toggle-collapse"),
                Text::new("collapse or expand the steps under a step"),
                Surface::Window,
                &const { [Trigger::Click(Element::new("collapse"))] },
            ),
            Self::InlineLink => Spec::new(
                Text::new("inline-link"),
                Text::new("show the tour a step links to inline under it"),
                Surface::Window,
                &const { [Trigger::Click(Element::new("inline"))] },
            ),
            Self::OpenLinkedTour => Spec::new(
                Text::new("open-linked-tour"),
                Text::new("open the tour a step links to, or a step that links here"),
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
                        Trigger::Click(Element::new("doc-hide-code")),
                        Trigger::Click(Element::new("doc-show-code")),
                        Trigger::Palette(Text::new("hide all code"), None),
                        Trigger::Palette(Text::new("show all code"), None),
                    ]
                },
            ),
            Self::CollapseAll => Spec::new(
                Text::new("collapse-all"),
                Text::new("collapse every step with children, or expand all"),
                Surface::Window,
                &const {
                    [
                        Trigger::Click(Element::new("doc-collapse")),
                        Trigger::Click(Element::new("doc-expand")),
                        Trigger::Palette(Text::new("collapse all"), None),
                        Trigger::Palette(Text::new("expand all"), None),
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
            Self::RemoveTour => Spec::new(
                Text::new("remove-tour"),
                Text::new("delete the tour being read"),
                Surface::Window,
                &const { [Trigger::Click(Element::new("doc-delete"))] },
            ),
            Self::ShowGraph => Spec::new(
                Text::new("show-graph"),
                Text::new("draw the tour being read as a graph"),
                Surface::Window,
                &const {
                    [
                        Trigger::Click(Element::new("doc-graph")),
                        Trigger::Palette(Text::new("show tour as graph"), None),
                    ]
                },
            ),
            Self::SwitchTab => Spec::new(
                Text::new("switch-tab"),
                Text::new("show a view by clicking its tab in its panel"),
                Surface::Window,
                &const { [Trigger::Click(Element::new("tab"))] },
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
                        Trigger::Palette(Text::new("go back"), Some(Chord::alt(Key::Left))),
                        Trigger::Palette(Text::new("go forward"), Some(Chord::alt(Key::Right))),
                    ]
                },
            ),
            Self::Save => Spec::new(
                Text::new("save"),
                Text::new("write the map's changed tours to disk"),
                Surface::Window,
                &const {
                    [
                        Trigger::Click(Element::new("save")),
                        Trigger::Key(Chord::control(Key::Letter(Letter::new('s')))),
                        Trigger::Palette(
                            Text::new("save"),
                            Some(Chord::control(Key::Letter(Letter::new('s')))),
                        ),
                    ]
                },
            ),
            Self::AddStep => Spec::new(
                Text::new("add-step"),
                Text::new(
                    "add the lines selected in the Source view, or the symbol in front of you, as a step of the tour being read",
                ),
                Surface::Window,
                &const {
                    [
                        Trigger::Click(Element::new("add-lines")),
                        Trigger::Click(Element::new("add-sym")),
                        Trigger::Click(Element::new("add-focus")),
                        Trigger::Click(Element::new("add-xto")),
                        Trigger::Click(Element::new("add-xfrom")),
                        Trigger::Click(Element::new("node-add")),
                        Trigger::Click(Element::new("add-offer")),
                        Trigger::Palette(Text::new("add step"), None),
                    ]
                },
            ),
            Self::ChooseTarget => Spec::new(
                Text::new("choose-target"),
                Text::new(
                    "add steps at the top level of the tour being read instead of under the selected step",
                ),
                Surface::Window,
                &const { [Trigger::Click(Element::new("target-top"))] },
            ),
            Self::MoveStep => Spec::new(
                Text::new("move-step"),
                Text::new(
                    "drag a step in the steps list onto another step to hang it there, or onto an edge to place it beside",
                ),
                Surface::Window,
                &const { [Trigger::Gesture(Gesture::Drag, Element::new("steps"))] },
            ),
            Self::PromoteSymbol => Spec::new(
                Text::new("promote-symbol"),
                Text::new("create a flow tour from the selected symbol and the symbols it calls"),
                Surface::Window,
                &const { [Trigger::Click(Element::new("promote-focus"))] },
            ),
            Self::SearchFiles => Spec::new(
                Text::new("search-files"),
                Text::new("search every file for a regex; the hits open in the Search tab"),
                Surface::Window,
                &const {
                    [
                        Trigger::Type(Element::new("field@search")),
                        Trigger::Click(Element::new("hit")),
                        Trigger::Click(Element::new("hitfile")),
                        Trigger::Palette(Text::new("search every file"), None),
                    ]
                },
            ),
            Self::RunCommand => Spec::new(
                Text::new("run-command"),
                Text::new("run a CLI command in the Console"),
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
            Self::FilterTours => Spec::new(
                Text::new("filter-tours"),
                Text::new(
                    "filter the Tours tab to the tours whose name, or a step's symbol or file, contains the text",
                ),
                Surface::Window,
                &const { [Trigger::Type(Element::new("field@tours"))] },
            ),
            Self::BrowseFiles => Spec::new(
                Text::new("browse-files"),
                Text::new(
                    "open a directory or a file in the Files tab, or a file from its row in Symbols",
                ),
                Surface::Window,
                &const {
                    [
                        Trigger::Click(Element::new("dir")),
                        Trigger::Click(Element::new("file")),
                        Trigger::Click(Element::new("symfile")),
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
                        Trigger::Click(Element::new("xreffile")),
                    ]
                },
            ),
            Self::GoToLine => Spec::new(
                Text::new("go-to-line"),
                Text::new("jump the Source view to a line number"),
                Surface::Window,
                &const { [Trigger::Type(Element::new("field@goto-line"))] },
            ),
            Self::SelectLines => Spec::new(
                Text::new("select-lines"),
                Text::new("select a line, or drag over lines to select them, in the Source view"),
                Surface::Window,
                &const {
                    [
                        Trigger::Click(Element::new("lines")),
                        Trigger::Gesture(Gesture::Drag, Element::new("lines")),
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
                Text::new(
                    "keep the definition of the identifier under the pointer above the references",
                ),
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
                &const {
                    [
                        Trigger::Gesture(Gesture::ShiftWheel, Element::new("code")),
                        Trigger::Gesture(Gesture::Swipe, Element::new("code")),
                        Trigger::Gesture(Gesture::Drag, Element::new("code-scrollbar")),
                    ]
                },
            ),
            Self::RevealNode => Spec::new(
                Text::new("reveal-node"),
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
                Text::new(
                    "show more of a graph node's file around it, cut it to its own lines, or cut it to a preview of its lines and back to whole",
                ),
                Surface::Window,
                &const {
                    [
                        Trigger::Click(Element::new("node-context")),
                        Trigger::Click(Element::new("node-preview")),
                    ]
                },
            ),
            Self::NodeSource => Spec::new(
                Text::new("node-source"),
                Text::new("open a graph node's file in the Source view"),
                Surface::Window,
                &const { [Trigger::Click(Element::new("node-source"))] },
            ),
            Self::ShowReferences => Spec::new(
                Text::new("show-references"),
                Text::new("open the References view on the graph node under the pointer"),
                Surface::Window,
                &const { [Trigger::Gesture(Gesture::DoubleClick, Element::new("node"))] },
            ),
            Self::AutoLayout => Spec::new(
                Text::new("auto-layout"),
                Text::new("lay the graph out again, dropping dragged positions"),
                Surface::Window,
                &const {
                    [
                        Trigger::Click(Element::new("graph-auto")),
                        Trigger::Palette(Text::new("auto layout graph"), None),
                    ]
                },
            ),
            Self::FitGraph => Spec::new(
                Text::new("fit-graph"),
                Text::new("zoom the graph to show all of it, or back to 1:1"),
                Surface::Window,
                &const {
                    [
                        Trigger::Click(Element::new("graph-fit")),
                        Trigger::Click(Element::new("graph-1to1")),
                        Trigger::Palette(Text::new("fit graph"), None),
                        Trigger::Palette(Text::new("graph at 1:1"), None),
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
                Text::new("zoom the graph around the pointer with a pinch or ctrl+wheel"),
                Surface::Window,
                &const {
                    [
                        Trigger::Gesture(Gesture::Pinch, Element::new("graph-canvas")),
                        Trigger::Gesture(Gesture::ControlWheel, Element::new("graph-canvas")),
                    ]
                },
            ),
            Self::WalkGraph => Spec::new(
                Text::new("walk-graph"),
                Text::new(
                    "after a click in the graph, move to the node left or right with the arrow keys, or to a box's parent with the button in the box's header; the camera glides to each node reached",
                ),
                Surface::Window,
                &const {
                    [
                        Trigger::Key(Chord::plain(Key::Left)),
                        Trigger::Key(Chord::plain(Key::Right)),
                        Trigger::Click(Element::new("box-parent")),
                    ]
                },
            ),
            Self::TurnGraph => Spec::new(
                Text::new("turn-graph"),
                Text::new("lay the graph out left to right or top to bottom"),
                Surface::Window,
                &const {
                    [
                        Trigger::Click(Element::new("graph-turn")),
                        Trigger::Palette(Text::new("turn graph direction"), None),
                    ]
                },
            ),
            Self::MoveView => Spec::new(
                Text::new("move-view"),
                Text::new(
                    "drag a view's tab onto another panel to join it, or onto a panel's edge to split it",
                ),
                Surface::Window,
                &const { [Trigger::Gesture(Gesture::Drag, Element::new("tab"))] },
            ),
            Self::ResizePanel => Spec::new(
                Text::new("resize-panel"),
                Text::new("drag the divider between two panels to resize them"),
                Surface::Window,
                &const { [Trigger::Gesture(Gesture::Drag, Element::new("divider"))] },
            ),
            Self::SplitPanel => Spec::new(
                Text::new("split-panel"),
                Text::new("split a panel in two, to the right or below"),
                Surface::Window,
                &const {
                    [
                        Trigger::Click(Element::new("split-right")),
                        Trigger::Click(Element::new("split-down")),
                        Trigger::Palette(Text::new("split panel right"), None),
                        Trigger::Palette(Text::new("split panel down"), None),
                    ]
                },
            ),
            Self::ClosePanel => Spec::new(
                Text::new("close-panel"),
                Text::new("close a panel; its neighbour takes its place"),
                Surface::Window,
                &const { [Trigger::Click(Element::new("close-panel"))] },
            ),
            Self::CloseTab => Spec::new(
                Text::new("close-tab"),
                Text::new("close one view's tab; a panel left with no tabs closes"),
                Surface::Window,
                &const { [Trigger::Click(Element::new("close-tab"))] },
            ),
            Self::PickView => Spec::new(
                Text::new("pick-view"),
                Text::new("choose a view for a panel from a searchable list"),
                Surface::Window,
                &const {
                    [
                        Trigger::Click(Element::new("pick")),
                        Trigger::Type(Element::new("field@views")),
                        Trigger::Click(Element::new("view")),
                    ]
                },
            ),
            Self::OpenDiffRow => Spec::new(
                Text::new("open-diff-row"),
                Text::new("open a tour from the map diff, or refresh the diff"),
                Surface::Window,
                &const {
                    [
                        Trigger::Click(Element::new("diffrow")),
                        Trigger::Click(Element::new("diff-refresh")),
                    ]
                },
            ),
            Self::CommandPalette => Spec::new(
                Text::new("palette"),
                Text::new(
                    "type to find a tour, step, symbol, file or view and go to it, or an action and run it",
                ),
                Surface::Window,
                &const {
                    [
                        Trigger::Key(Chord::control(Key::Letter(Letter::new('p')))),
                        Trigger::Type(Element::new("field@palette")),
                        Trigger::Click(Element::new("palette")),
                    ]
                },
            ),
            Self::Welcome => Spec::new(
                Text::new("welcome"),
                Text::new(
                    "the Tour view with nothing chosen: the map's size and coverage, the tours changed since the parent revision, the groups to start reading, the keys",
                ),
                Surface::Window,
                &const {
                    [
                        Trigger::Click(Element::new("start-change")),
                        Trigger::Click(Element::new("start-more-changes")),
                        Trigger::Click(Element::new("start-group")),
                        Trigger::Click(Element::new("start-ungrouped")),
                        Trigger::Click(Element::new("start-uncovered")),
                        Trigger::Click(Element::new("start-root")),
                        Trigger::Palette(Text::new("start page"), None),
                    ]
                },
            ),
            Self::BuildTour => Spec::new(
                Text::new("build-tour"),
                Text::new(
                    "a wizard drawn in the Tour view that builds a tour page by page: name and kind, the symbol it starts from, the steps from its call tree opened as deep as wanted with promote's verdict on every call, the note",
                ),
                Surface::Window,
                &const {
                    [
                        Trigger::Click(Element::new("build-tour")),
                        Trigger::Click(Element::new("new-tour")),
                        Trigger::Click(Element::new("tour-from-here")),
                        Trigger::Click(Element::new("wizard-back")),
                        Trigger::Click(Element::new("wizard-next")),
                        Trigger::Click(Element::new("wizard-cancel")),
                        Trigger::Click(Element::new("wizard-create")),
                        Trigger::Click(Element::new("wizard-kind")),
                        Trigger::Click(Element::new("wizard-sym")),
                        Trigger::Click(Element::new("wizard-tick")),
                        Trigger::Click(Element::new("wizard-use-focus")),
                        Trigger::Click(Element::new("wizard-open")),
                        Trigger::Click(Element::new("wizard-fold")),
                        Trigger::Click(Element::new("wizard-return")),
                        Trigger::Type(Element::new("field@wizard-name")),
                        Trigger::Type(Element::new("field@wizard-group")),
                        Trigger::Type(Element::new("field@wizard-search")),
                        Trigger::Type(Element::new("field@tour-note")),
                        Trigger::Palette(Text::new("build a tour"), None),
                    ]
                },
            ),
            Self::EditTour => Spec::new(
                Text::new("edit-tour"),
                Text::new(
                    "one page in the Tour view that edits the tour being read: its name, kind, group and note, every step's note in place, steps removed by unticking them and calls added by opening a step, with the changes listed before they are applied together",
                ),
                Surface::Window,
                &const {
                    [
                        Trigger::Click(Element::new("doc-edit")),
                        Trigger::Click(Element::new("edit-apply")),
                        Trigger::Click(Element::new("edit-cancel")),
                        Trigger::Click(Element::new("edit-more")),
                        Trigger::Type(Element::new("edit-note")),
                        Trigger::Palette(Text::new("edit tour"), None),
                    ]
                },
            ),
            Self::EditText => Spec::new(
                Text::new("edit-text"),
                Text::new(
                    "how every text field edits: the arrows, home and end move the caret, shift selects, alt or ctrl moves and deletes by word, a click places the caret and a drag or a double click selects, ctrl+c, ctrl+x and ctrl+v copy, cut and paste; in the tour note enter breaks the line, ctrl+enter goes on and the arrows move by row",
                ),
                Surface::Window,
                &const {
                    [
                        Trigger::Key(Chord::control(Key::Letter(Letter::new('c')))),
                        Trigger::Key(Chord::control(Key::Letter(Letter::new('x')))),
                        Trigger::Key(Chord::control(Key::Letter(Letter::new('v')))),
                        Trigger::Gesture(Gesture::Drag, Element::new("field@tour-note")),
                        Trigger::Gesture(Gesture::ShiftClick, Element::new("field@tour-note")),
                        Trigger::Gesture(Gesture::DoubleClick, Element::new("field@tour-note")),
                        Trigger::Gesture(Gesture::Wheel, Element::new("field@tour-note")),
                        Trigger::Gesture(Gesture::Drag, Element::new("edit-note")),
                        Trigger::Gesture(Gesture::ShiftClick, Element::new("edit-note")),
                        Trigger::Gesture(Gesture::DoubleClick, Element::new("edit-note")),
                        Trigger::Gesture(Gesture::Drag, Element::new("field@wizard-name")),
                        Trigger::Gesture(Gesture::Drag, Element::new("field@wizard-group")),
                        Trigger::Gesture(Gesture::Drag, Element::new("field@wizard-search")),
                        Trigger::Gesture(Gesture::Drag, Element::new("field@search")),
                        Trigger::Gesture(Gesture::Drag, Element::new("field@symbols")),
                        Trigger::Gesture(Gesture::Drag, Element::new("field@tours")),
                        Trigger::Gesture(Gesture::Drag, Element::new("field@goto-line")),
                        Trigger::Gesture(Gesture::Drag, Element::new("field@views")),
                        Trigger::Gesture(Gesture::Drag, Element::new("field@palette")),
                    ]
                },
            ),
            Self::Settings => Spec::new(
                Text::new("settings"),
                Text::new(
                    "a box over the window to choose the theme, the font, the font size and which way the graph grows; kept per user across launches",
                ),
                Surface::Window,
                &const {
                    [
                        Trigger::Key(Chord::control(Key::Letter(Letter::new(',')))),
                        Trigger::Click(Element::new("settings-open")),
                        Trigger::Click(Element::new("settings-close")),
                        Trigger::Click(Element::new("settings-theme")),
                        Trigger::Click(Element::new("settings-smaller")),
                        Trigger::Click(Element::new("settings-larger")),
                        Trigger::Click(Element::new("settings-font")),
                        Trigger::Click(Element::new("settings-graph")),
                        Trigger::Palette(
                            Text::new("settings"),
                            Some(Chord::control(Key::Letter(Letter::new(',')))),
                        ),
                    ]
                },
            ),
            Self::ElementTip => Spec::new(
                Text::new("element-tip"),
                Text::new(
                    "rest the pointer on a button to read what it does and the key that does the same",
                ),
                Surface::Window,
                &const {
                    [Trigger::Gesture(
                        Gesture::Hover,
                        Element::new("element-tip"),
                    )]
                },
            ),
        }
    }
}

#[cfg(test)]
mod tests;
