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

    #[cfg(test)]
    pub(crate) const fn element(self) -> Element {
        self.element
    }
}

pub(crate) const SEARCH_FIELD: Control =
    Control::new(Feature::SearchFiles, Element::new("field@search"));
pub(crate) const NEW_TOUR_FIELD: Control =
    Control::new(Feature::NewTour, Element::new("field@new-tour"));
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
pub(crate) const NEW_GROUP_FIELD: Control =
    Control::new(Feature::NewTour, Element::new("field@new-group"));
pub(crate) const NEW_TOUR: Control = Control::new(Feature::NewTour, Element::new("new-tour"));
pub(crate) const KIND: Control = Control::new(Feature::NewTour, Element::new("kind"));
pub(crate) const CREATE_TOUR: Control = Control::new(Feature::NewTour, Element::new("create-tour"));
pub(crate) const ADD_LINES: Control = Control::new(Feature::AddStep, Element::new("add-lines"));
pub(crate) const ADD_SYMBOL: Control = Control::new(Feature::AddStep, Element::new("add-sym"));
pub(crate) const ADD_FOCUS: Control = Control::new(Feature::AddStep, Element::new("add-focus"));
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
    Control::new(Feature::HideAllCode, Element::new("doc-hide-code"));
pub(crate) const SHOW_ALL_CODE: Control =
    Control::new(Feature::HideAllCode, Element::new("doc-show-code"));
pub(crate) const COLLAPSE_ALL: Control =
    Control::new(Feature::CollapseAll, Element::new("doc-collapse"));
pub(crate) const EXPAND_ALL: Control =
    Control::new(Feature::CollapseAll, Element::new("doc-expand"));
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

#[cfg(test)]
pub(crate) const CONTROLS: &[Control] = &[
    SEARCH_FIELD,
    NEW_TOUR_FIELD,
    FILTER_FIELD,
    TOUR_FILTER_FIELD,
    LINE_FIELD,
    COMMAND_FIELD,
    BACK,
    FORWARD,
    TAB,
    NEW_GROUP_FIELD,
    NEW_TOUR,
    KIND,
    CREATE_TOUR,
    ADD_LINES,
    ADD_SYMBOL,
    ADD_FOCUS,
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
    SHOW_ALL_CODE,
    COLLAPSE_ALL,
    EXPAND_ALL,
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

pub(crate) fn tour_header() -> Id {
    Id::new("tour-header")
}

pub(crate) fn tour_buttons() -> Id {
    Id::new("tour-buttons")
}

pub(crate) fn palette_box() -> Id {
    Id::new("palette-box")
}
