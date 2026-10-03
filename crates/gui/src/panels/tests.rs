use super::*;

fn at(horizontal: i32, vertical: i32) -> Point {
    Point::new(Px::new(horizontal), Px::new(vertical))
}

fn rect(left: i32, top: i32, width: i32, height: i32) -> Rect {
    Rect::new(Px::new(left), Px::new(top), Px::new(width), Px::new(height))
}

const DEFAULT: &str = "down6(820 right5(220 0[Tours* Symbols Files] right4(740 1[Tour* Diff Graph Source Search] 2[References*])) 3[Console*])";

#[test]
fn a_bare_panel_holds_no_view() {
    let panels = Panels::bare();
    assert!(panels.is_bare());
    assert_eq!(panels.to_string(), "0[]");
    assert!(!Panels::default().is_bare());
}

#[test]
fn the_default_tree_holds_every_view_once() {
    let panels = Panels::default();
    assert_eq!(panels.to_string(), DEFAULT);
    for view in View::VARIANTS {
        assert_eq!(
            panels
                .panels()
                .iter()
                .filter(|panel| panel.holds(*view))
                .count(),
            1,
            "{view:?}"
        );
    }
}

#[test]
fn a_split_opens_the_picker_on_an_empty_panel_and_a_pick_moves_the_view() {
    let mut panels = Panels::default();
    panels.split_panel(BranchId(1), Direction::Right);
    assert_eq!(panels.picker(), Some(BranchId(7)));
    panels.pick(BranchId(7), View::Source);
    assert_eq!(panels.picker(), None);
    assert_eq!(
        panels.to_string(),
        "down6(820 right5(220 0[Tours* Symbols Files] right4(740 right8(500 1[Tour* Diff Graph Search] 7[Source*]) 2[References*])) 3[Console*])"
    );
}

#[test]
fn taking_the_last_view_from_a_panel_closes_it() {
    let mut panels = Panels::default();
    panels.put(View::References, BranchId(0));
    assert_eq!(
        panels.to_string(),
        "down6(820 right5(220 0[Tours Symbols Files References*] 1[Tour* Diff Graph Source Search]) 3[Console*])"
    );
}

#[test]
fn a_drop_on_an_edge_splits_the_panel_on_that_side() {
    let mut panels = Panels::default();
    let target = DropTarget {
        panel: BranchId(0),
        zone: Zone::Edge(Edge::Bottom),
        band: rect(0, 0, 1, 1),
    };
    panels.drop_view(View::Console, target);
    assert_eq!(
        panels.to_string(),
        "right5(220 down8(500 0[Tours* Symbols Files] 7[Console*]) right4(740 1[Tour* Diff Graph Source Search] 2[References*]))"
    );
    let alone = DropTarget {
        panel: BranchId(7),
        zone: Zone::Edge(Edge::Left),
        band: rect(0, 0, 1, 1),
    };
    let before = panels.to_string();
    panels.drop_view(View::Console, alone);
    assert_eq!(panels.to_string(), before);
}

#[test]
fn closing_a_panel_hands_its_place_to_its_sibling() {
    let mut panels = Panels::default();
    panels.close(BranchId(3));
    panels.close(BranchId(2));
    assert_eq!(
        panels.to_string(),
        "right5(220 0[Tours* Symbols Files] 1[Tour* Diff Graph Source Search])"
    );
    panels.close(BranchId(0));
    assert!(panels.is_single());
    panels.close(BranchId(1));
    assert!(panels.is_single());
}

#[test]
fn navigation_reveals_a_hidden_view_next_to_the_last_one_it_showed() {
    let mut panels = Panels::default();
    panels.close(BranchId(1));
    panels.reveal(Tab::Source, Ticket::default());
    assert_eq!(panels.holder(View::Source), None);
    let mut asked = Ticket::default();
    asked = asked.next();
    panels.reveal(Tab::Source, asked);
    assert_eq!(panels.holder(View::Source), Some(BranchId(0)));
    assert!(panels.is_shown(View::Source));
    panels.activate(View::Tours);
    panels.reveal(Tab::Source, asked);
    assert!(panels.is_shown(View::Tours));
    panels.reveal(Tab::Source, asked.next());
    assert!(panels.is_shown(View::Source));
}

#[test]
fn a_division_keeps_both_halves_above_the_floor() {
    let panels = Panels::default();
    let Branch::Split(root) = panels.root() else {
        panic!("the default root is a split");
    };
    let least = Extent::new(Px::new(96), Px::new(64));
    let divided = root.divide(rect(0, 30, 1600, 940), Px::new(8), least);
    assert_eq!(divided.first, rect(0, 30, 1600, 764));
    assert_eq!(divided.divider, rect(0, 794, 1600, 8));
    assert_eq!(divided.second, rect(0, 802, 1600, 168));
    let mut squeezed = panels.clone();
    squeezed.resize(BranchId(6), Ratio::permille(990));
    let Branch::Split(squeezed_root) = squeezed.root() else {
        panic!("still a split");
    };
    let squeezed_halves = squeezed_root.divide(rect(0, 30, 1600, 940), Px::new(8), least);
    assert_eq!(squeezed_halves.second.height, Px::new(64));
}

#[test]
fn the_nearest_edge_within_a_quarter_picks_the_side() {
    let panel = rect(0, 0, 400, 400);
    assert_eq!(zone(panel, at(200, 200)), Zone::Middle);
    assert_eq!(zone(panel, at(20, 200)), Zone::Edge(Edge::Left));
    assert_eq!(zone(panel, at(200, 390)), Zone::Edge(Edge::Bottom));
    assert_eq!(band(panel, Zone::Edge(Edge::Right)), rect(200, 0, 200, 400));
}

#[test]
fn a_grab_moves_once_the_pointer_leaves_its_reach() {
    let mut panels = Panels::default();
    panels.take_hold(View::Tours, at(10, 10));
    panels.drag_to(at(14, 14));
    assert!(!panels.grab().unwrap().is_moving());
    panels.drag_to(at(15, 14));
    assert!(panels.grab().unwrap().is_moving());
}

#[test]
fn views_are_found_by_name_in_any_case() {
    assert_eq!(View::named(&Label::new("source")), Some(View::Source));
    assert_eq!(
        View::named(&Label::new("REFERENCES")),
        Some(View::References)
    );
    assert_eq!(View::named(&Label::new("nothing")), None);
}

#[test]
fn a_tree_survives_its_saved_layout() {
    let mut panels = Panels::default();
    panels.split_panel(BranchId(1), Direction::Down);
    panels.pick(BranchId(7), View::Graph);
    panels.resize(BranchId(6), Ratio::permille(700));
    let restored = Panels::from_layout(&panels.layout());
    assert_eq!(restored.layout(), panels.layout());
    assert_eq!(
        restored.to_string(),
        "down8(700 right6(220 0[Tours* Symbols Files] right5(740 down3(500 1[Tour* Diff Source Search] 2[Graph*]) 4[References*])) 7[Console*])"
    );
}

#[test]
fn a_saved_layout_drops_unknown_and_repeated_views() {
    let key = |name: &str| ViewKey::new(name).unwrap();
    let saved = LayoutTree::Split(LayoutSplit::new(
        SplitDirection::Right,
        Share::permille(300).unwrap(),
        LayoutTree::Panel(LayoutPanel::new(
            vec![key("Tours"), key("Gone"), key("Graph")],
            Some(key("Graph")),
        )),
        LayoutTree::Panel(LayoutPanel::new(vec![key("Tours"), key("Console")], None)),
    ));
    assert_eq!(
        Panels::from_layout(&saved).to_string(),
        "right2(300 0[Tours Graph*] 1[Console*])"
    );
}

#[test]
fn closing_a_panels_last_tab_closes_the_panel_but_never_the_last_panel() {
    let mut panels = Panels::default();
    panels.close_view(View::Diff);
    panels.close_view(View::References);
    assert_eq!(
        panels.to_string(),
        "down6(820 right5(220 0[Tours* Symbols Files] 1[Tour* Graph Source Search]) 3[Console*])"
    );
    for view in View::VARIANTS {
        panels.close_view(*view);
    }
    assert!(panels.is_single());
    assert_eq!(panels.panels().len(), 1);
    for panel in panels.panels() {
        assert_eq!(panel.views().len(), 0);
    }
}

#[test]
fn a_closed_view_is_brought_back_in_a_split_beside_the_view_it_serves() {
    let mut panels = Panels::default();
    panels.close_view(View::References);
    panels.bring(View::References, View::Source);
    assert_eq!(
        panels.to_string(),
        "down6(820 right5(220 0[Tours* Symbols Files] right8(500 1[Tour* Diff Graph Source Search] 7[References*])) 3[Console*])"
    );
    panels.activate(View::Symbols);
    panels.bring(View::Tours, View::Source);
    assert!(panels.is_shown(View::Tours));
    assert_eq!(panels.holder(View::Tours), Some(BranchId(0)));
}
