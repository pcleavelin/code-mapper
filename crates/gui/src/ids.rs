use features::{Element, Feature};
use ui::{Count, Id, Label};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Target(Id);

impl Target {
    pub(crate) const fn id(self) -> Id {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Control {
    feature: Feature,
    element: Element,
}

impl Control {
    const fn new(feature: Feature, element: Element) -> Self {
        Self { feature, element }
    }

    pub(crate) fn id(self) -> Id {
        Id::from_name(self.element.as_str())
    }

    pub(crate) fn target(self) -> Target {
        Target(self.id())
    }

    pub(crate) fn nth(self, number: Count) -> Target {
        Target(self.id().nth(number.get()))
    }

    pub(crate) fn with(self, suffix: &Label) -> Target {
        Target(self.id().with(suffix.as_str()))
    }

    pub(crate) fn nested(self, occurrence: Count, number: Count) -> Target {
        Target(
            linked()
                .nth(occurrence.get())
                .with(self.element.as_str())
                .nth(number.get()),
        )
    }

    #[cfg(test)]
    pub(crate) const fn feature(self) -> Feature {
        self.feature
    }

    pub(crate) const fn element(self) -> Element {
        self.element
    }
}

pub(crate) const SEARCH_FIELD: Control =
    Control::new(Feature::SearchFiles, Element::new("field@search"));
pub(crate) const FILTER_FIELD: Control =
    Control::new(Feature::FilterSymbols, Element::new("field@symbols"));
pub(crate) const TOUR_FILTER_FIELD: Control =
    Control::new(Feature::FilterTours, Element::new("field@tours"));
pub(crate) const LINE_FIELD: Control =
    Control::new(Feature::GoToLine, Element::new("field@goto-line"));
pub(crate) const COMMAND_FIELD: Control = Control::new(Feature::RunCommand, Element::new("cmd"));
pub(crate) const BACK: Control = Control::new(Feature::GoBack, Element::new("back"));
pub(crate) const FORWARD: Control = Control::new(Feature::GoBack, Element::new("forward"));
pub(crate) const TAB: Control = Control::new(Feature::SwitchTab, Element::new("tab"));
pub(crate) const NEW_TOUR: Control = Control::new(Feature::BuildTour, Element::new("new-tour"));
pub(crate) const ADD_LINES: Control = Control::new(Feature::AddStep, Element::new("add-lines"));
pub(crate) const ADD_SYMBOL: Control = Control::new(Feature::AddStep, Element::new("add-sym"));
pub(crate) const ADD_FOCUS: Control = Control::new(Feature::AddStep, Element::new("add-focus"));
pub(crate) const ADD_OFFER: Control = Control::new(Feature::AddStep, Element::new("add-offer"));
pub(crate) const ADD_CALLER: Control = Control::new(Feature::AddStep, Element::new("add-xto"));
pub(crate) const ADD_CALLEE: Control = Control::new(Feature::AddStep, Element::new("add-xfrom"));
pub(crate) const TARGET_TOP: Control =
    Control::new(Feature::ChooseTarget, Element::new("target-top"));
pub(crate) const PROMOTE_FOCUS: Control =
    Control::new(Feature::PromoteSymbol, Element::new("promote-focus"));
pub(crate) const SAVE: Control = Control::new(Feature::Save, Element::new("save"));
pub(crate) const DIVIDER: Control = Control::new(Feature::ResizePanel, Element::new("divider"));
pub(crate) const SPLIT_RIGHT: Control =
    Control::new(Feature::SplitPanel, Element::new("split-right"));
pub(crate) const SPLIT_DOWN: Control =
    Control::new(Feature::SplitPanel, Element::new("split-down"));
pub(crate) const CLOSE_PANEL: Control =
    Control::new(Feature::ClosePanel, Element::new("close-panel"));
pub(crate) const CLOSE_TAB: Control = Control::new(Feature::CloseTab, Element::new("close-tab"));
pub(crate) const PICK: Control = Control::new(Feature::PickView, Element::new("pick"));
pub(crate) const VIEW_ROW: Control = Control::new(Feature::PickView, Element::new("view"));
pub(crate) const VIEW_FIELD: Control = Control::new(Feature::PickView, Element::new("field@views"));
pub(crate) const TOUR_ROW: Control = Control::new(Feature::OpenTour, Element::new("tours"));
pub(crate) const GROUP_ROW: Control = Control::new(Feature::OpenGroup, Element::new("group"));
pub(crate) const STEP_LIST_ROW: Control = Control::new(Feature::SelectStep, Element::new("steps"));
pub(crate) const STEP_HEADER: Control = Control::new(Feature::SelectStep, Element::new("step"));
pub(crate) const CRUMB: Control = Control::new(Feature::SelectStep, Element::new("crumb"));
pub(crate) const FOUND_STEP: Control = Control::new(Feature::SelectStep, Element::new("found"));
pub(crate) const SYMBOL_ROW: Control = Control::new(Feature::FilterSymbols, Element::new("sym"));
pub(crate) const SYMBOL_FILE_ROW: Control =
    Control::new(Feature::BrowseFiles, Element::new("symfile"));
pub(crate) const DIRECTORY_ROW: Control = Control::new(Feature::BrowseFiles, Element::new("dir"));
pub(crate) const FILE_ROW: Control = Control::new(Feature::BrowseFiles, Element::new("file"));
pub(crate) const CALLER_ROW: Control = Control::new(Feature::FollowXref, Element::new("xto"));
pub(crate) const CALLEE_ROW: Control = Control::new(Feature::FollowXref, Element::new("xfrom"));
pub(crate) const REFERENCE_ROW: Control = Control::new(Feature::FollowXref, Element::new("xref"));
pub(crate) const REFERENCE_FILE_ROW: Control =
    Control::new(Feature::FollowXref, Element::new("xreffile"));
pub(crate) const PEEK_GO: Control = Control::new(Feature::PeekDefinition, Element::new("peek-go"));
pub(crate) const PEEK_CLOSE: Control =
    Control::new(Feature::PeekDefinition, Element::new("peek-x"));
pub(crate) const SHOW_GRAPH: Control = Control::new(Feature::ShowGraph, Element::new("doc-graph"));
pub(crate) const HIDE_ALL_CODE: Control =
    Control::new(Feature::HideAllCode, Element::new("doc-code"));
pub(crate) const COLLAPSE_ALL: Control =
    Control::new(Feature::CollapseAll, Element::new("doc-collapse"));
pub(crate) const TOUR_MENU: Control = Control::new(Feature::RemoveTour, Element::new("doc-menu"));
pub(crate) const REMOVE_TOUR: Control =
    Control::new(Feature::RemoveTour, Element::new("doc-delete"));
pub(crate) const LINKED_FROM: Control = Control::new(Feature::OpenLinkedTour, Element::new("from"));
pub(crate) const COLLAPSE: Control =
    Control::new(Feature::ToggleCollapse, Element::new("collapse"));
pub(crate) const HIDE_CODE: Control = Control::new(Feature::ToggleCode, Element::new("hide"));
pub(crate) const WHOLE: Control = Control::new(Feature::ToggleWholeSymbol, Element::new("whole"));
pub(crate) const NO_CONTEXT: Control = Control::new(Feature::MoreContext, Element::new("ctx0"));
pub(crate) const CONTEXT_ABOVE: Control = Control::new(Feature::MoreContext, Element::new("ctx-a"));
pub(crate) const CONTEXT_BELOW: Control = Control::new(Feature::MoreContext, Element::new("ctx-b"));
pub(crate) const LINK: Control = Control::new(Feature::OpenLinkedTour, Element::new("link"));
pub(crate) const INLINE: Control = Control::new(Feature::InlineLink, Element::new("inline"));
pub(crate) const REMOVE_STEP: Control = Control::new(Feature::RemoveStep, Element::new("del"));
pub(crate) const LINES: Control = Control::new(Feature::SelectLines, Element::new("lines"));
pub(crate) const HIT_ROW: Control = Control::new(Feature::SearchFiles, Element::new("hit"));
pub(crate) const HIT_FILE_ROW: Control =
    Control::new(Feature::SearchFiles, Element::new("hitfile"));
pub(crate) const DIFF_REFRESH: Control =
    Control::new(Feature::OpenDiffRow, Element::new("diff-refresh"));
pub(crate) const DIFF_ROW: Control = Control::new(Feature::OpenDiffRow, Element::new("diffrow"));
pub(crate) const GRAPH_ONE_TO_ONE: Control =
    Control::new(Feature::FitGraph, Element::new("graph-1to1"));
pub(crate) const GRAPH_AUTO: Control =
    Control::new(Feature::AutoLayout, Element::new("graph-auto"));
pub(crate) const GRAPH_FIT: Control = Control::new(Feature::FitGraph, Element::new("graph-fit"));
pub(crate) const GRAPH_TURN: Control = Control::new(Feature::TurnGraph, Element::new("graph-turn"));
pub(crate) const GRAPH_CANVAS: Control =
    Control::new(Feature::PanGraph, Element::new("graph-canvas"));
pub(crate) const PALETTE_FIELD: Control =
    Control::new(Feature::CommandPalette, Element::new("field@palette"));
pub(crate) const PALETTE_ROW: Control =
    Control::new(Feature::CommandPalette, Element::new("palette"));
pub(crate) const SETTINGS_OPEN: Control =
    Control::new(Feature::Settings, Element::new("settings-open"));
pub(crate) const SETTINGS_CLOSE: Control =
    Control::new(Feature::Settings, Element::new("settings-close"));
pub(crate) const SETTINGS_THEME: Control =
    Control::new(Feature::Settings, Element::new("settings-theme"));
pub(crate) const SETTINGS_SMALLER: Control =
    Control::new(Feature::Settings, Element::new("settings-smaller"));
pub(crate) const SETTINGS_LARGER: Control =
    Control::new(Feature::Settings, Element::new("settings-larger"));
pub(crate) const SETTINGS_FONT: Control =
    Control::new(Feature::Settings, Element::new("settings-font"));
pub(crate) const SETTINGS_GRAPH: Control =
    Control::new(Feature::Settings, Element::new("settings-graph"));
pub(crate) const BUILD_TOUR: Control = Control::new(Feature::BuildTour, Element::new("build-tour"));
pub(crate) const START_CHANGE: Control =
    Control::new(Feature::Welcome, Element::new("start-change"));
pub(crate) const START_MORE_CHANGES: Control =
    Control::new(Feature::Welcome, Element::new("start-more-changes"));
pub(crate) const START_GROUP: Control = Control::new(Feature::Welcome, Element::new("start-group"));
pub(crate) const START_UNGROUPED: Control =
    Control::new(Feature::Welcome, Element::new("start-ungrouped"));
pub(crate) const START_UNCOVERED: Control =
    Control::new(Feature::Welcome, Element::new("start-uncovered"));
pub(crate) const START_ROOT: Control = Control::new(Feature::Welcome, Element::new("start-root"));
pub(crate) const WIZARD_NAME_FIELD: Control =
    Control::new(Feature::BuildTour, Element::new("field@wizard-name"));
pub(crate) const WIZARD_GROUP_FIELD: Control =
    Control::new(Feature::BuildTour, Element::new("field@wizard-group"));
pub(crate) const WIZARD_SEARCH_FIELD: Control =
    Control::new(Feature::BuildTour, Element::new("field@wizard-search"));
pub(crate) const TOUR_NOTE_FIELD: Control =
    Control::new(Feature::BuildTour, Element::new("field@tour-note"));
pub(crate) const WIZARD_BACK: Control =
    Control::new(Feature::BuildTour, Element::new("wizard-back"));
pub(crate) const WIZARD_NEXT: Control =
    Control::new(Feature::BuildTour, Element::new("wizard-next"));
pub(crate) const WIZARD_CANCEL: Control =
    Control::new(Feature::BuildTour, Element::new("wizard-cancel"));
pub(crate) const WIZARD_CREATE: Control =
    Control::new(Feature::BuildTour, Element::new("wizard-create"));
pub(crate) const WIZARD_KIND: Control =
    Control::new(Feature::BuildTour, Element::new("wizard-kind"));
pub(crate) const WIZARD_SYMBOL: Control =
    Control::new(Feature::BuildTour, Element::new("wizard-sym"));
pub(crate) const WIZARD_TICK: Control =
    Control::new(Feature::BuildTour, Element::new("wizard-tick"));
pub(crate) const WIZARD_USE_FOCUS: Control =
    Control::new(Feature::BuildTour, Element::new("wizard-use-focus"));
pub(crate) const WIZARD_OPEN: Control =
    Control::new(Feature::BuildTour, Element::new("wizard-open"));
pub(crate) const WIZARD_FOLD: Control =
    Control::new(Feature::BuildTour, Element::new("wizard-fold"));
pub(crate) const WIZARD_RETURN: Control =
    Control::new(Feature::BuildTour, Element::new("wizard-return"));
pub(crate) const WIZARD_FROM_HERE: Control =
    Control::new(Feature::BuildTour, Element::new("tour-from-here"));
pub(crate) const EDIT_TOUR: Control = Control::new(Feature::EditTour, Element::new("doc-edit"));
pub(crate) const EDIT_APPLY: Control = Control::new(Feature::EditTour, Element::new("edit-apply"));
pub(crate) const EDIT_CANCEL: Control =
    Control::new(Feature::EditTour, Element::new("edit-cancel"));
pub(crate) const EDIT_MORE: Control = Control::new(Feature::EditTour, Element::new("edit-more"));
pub(crate) const EDIT_NOTE_FIELD: Control =
    Control::new(Feature::EditTour, Element::new("edit-note"));

#[cfg(test)]
pub(crate) const CONTROLS: &[Control] = &[
    SEARCH_FIELD,
    FILTER_FIELD,
    TOUR_FILTER_FIELD,
    LINE_FIELD,
    COMMAND_FIELD,
    BACK,
    FORWARD,
    TAB,
    NEW_TOUR,
    ADD_LINES,
    ADD_SYMBOL,
    ADD_FOCUS,
    ADD_OFFER,
    ADD_CALLER,
    ADD_CALLEE,
    TARGET_TOP,
    PROMOTE_FOCUS,
    SAVE,
    DIVIDER,
    SPLIT_RIGHT,
    SPLIT_DOWN,
    CLOSE_PANEL,
    CLOSE_TAB,
    PICK,
    VIEW_ROW,
    VIEW_FIELD,
    TOUR_ROW,
    GROUP_ROW,
    STEP_LIST_ROW,
    STEP_HEADER,
    CRUMB,
    FOUND_STEP,
    SYMBOL_ROW,
    SYMBOL_FILE_ROW,
    DIRECTORY_ROW,
    FILE_ROW,
    CALLER_ROW,
    CALLEE_ROW,
    REFERENCE_ROW,
    REFERENCE_FILE_ROW,
    PEEK_GO,
    PEEK_CLOSE,
    SHOW_GRAPH,
    HIDE_ALL_CODE,
    COLLAPSE_ALL,
    TOUR_MENU,
    REMOVE_TOUR,
    LINKED_FROM,
    COLLAPSE,
    HIDE_CODE,
    WHOLE,
    NO_CONTEXT,
    CONTEXT_ABOVE,
    CONTEXT_BELOW,
    LINK,
    INLINE,
    REMOVE_STEP,
    LINES,
    HIT_ROW,
    HIT_FILE_ROW,
    DIFF_REFRESH,
    DIFF_ROW,
    GRAPH_ONE_TO_ONE,
    GRAPH_AUTO,
    GRAPH_FIT,
    GRAPH_TURN,
    GRAPH_CANVAS,
    PALETTE_FIELD,
    PALETTE_ROW,
    BUILD_TOUR,
    START_CHANGE,
    START_MORE_CHANGES,
    START_GROUP,
    START_UNGROUPED,
    START_UNCOVERED,
    START_ROOT,
    WIZARD_NAME_FIELD,
    WIZARD_GROUP_FIELD,
    WIZARD_SEARCH_FIELD,
    TOUR_NOTE_FIELD,
    WIZARD_BACK,
    WIZARD_NEXT,
    WIZARD_CANCEL,
    WIZARD_CREATE,
    WIZARD_KIND,
    WIZARD_SYMBOL,
    WIZARD_TICK,
    WIZARD_USE_FOCUS,
    WIZARD_OPEN,
    WIZARD_FOLD,
    WIZARD_RETURN,
    WIZARD_FROM_HERE,
    EDIT_TOUR,
    EDIT_APPLY,
    EDIT_CANCEL,
    EDIT_MORE,
    EDIT_NOTE_FIELD,
    SETTINGS_OPEN,
    SETTINGS_CLOSE,
    SETTINGS_THEME,
    SETTINGS_SMALLER,
    SETTINGS_LARGER,
    SETTINGS_FONT,
    SETTINGS_GRAPH,
];

pub(crate) fn body() -> Id {
    Id::new("body")
}

pub(crate) fn pair() -> Id {
    Id::new("pair")
}

pub(crate) fn panel() -> Id {
    Id::new("panel")
}

pub(crate) fn tours() -> Id {
    TOUR_ROW.id()
}

pub(crate) fn symbols() -> Id {
    Id::new("symbols")
}

pub(crate) fn files() -> Id {
    Id::new("files")
}

pub(crate) fn references() -> Id {
    Id::new("references")
}

pub(crate) fn peek() -> Id {
    Id::new("peek")
}

pub(crate) fn peek_code() -> Id {
    Id::new("peekcode")
}

pub(crate) fn peek_outside() -> Id {
    Id::new("peekout")
}

pub(crate) fn console() -> Id {
    Id::new("console")
}

pub(crate) fn document() -> Id {
    Id::new("document")
}

pub(crate) fn source() -> Id {
    Id::new("source")
}

pub(crate) fn search() -> Id {
    Id::new("search")
}

pub(crate) fn diff() -> Id {
    Id::new("diff")
}

pub(crate) const DOCUMENT_CODE: Element = Element::new("doccode");
pub(crate) const STEP_COLUMN: Element = Element::new("stepcolumn");

pub(crate) fn linked() -> Id {
    Id::new("linked")
}

pub(crate) fn tooltip_code() -> Id {
    Id::new("tip-code")
}

pub(crate) fn picker() -> Id {
    Id::new("picker")
}

pub(crate) fn menu_box() -> Id {
    Id::new("menu")
}

pub(crate) fn tour_header() -> Id {
    Id::new("tour-header")
}

pub(crate) fn tour_buttons() -> Id {
    Id::new("tour-buttons")
}

pub(crate) fn palette_box() -> Id {
    Id::new("palette-box")
}

pub(crate) fn settings_box() -> Id {
    Id::new("settings-box")
}

pub(crate) fn start_page() -> Id {
    Id::new("start-page")
}

pub(crate) fn wizard() -> Id {
    Id::new("wizard")
}
