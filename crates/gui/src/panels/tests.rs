use super::*;

fn at(horizontal: i32, vertical: i32) -> Point {
    Point::new(Px::new(horizontal), Px::new(vertical))
}

fn rect(left: i32, top: i32, width: i32, height: i32) -> Rect {
    Rect::new(Px::new(left), Px::new(top), Px::new(width), Px::new(height))
}

const DEFAULT: &str = "down6(820 across5(220 0[Paths* Symbols Files] across4(740 1[Path* Diff Graph Listing Results] 2[Xrefs*])) 3[Output*])";

#[test]
fn the_default_tree_holds_every_view_once() {
    let panels = Panels::default();
    assert_eq!(panels.to_string(), DEFAULT);
    for view in View::ALL {
        assert_eq!(
            panels
                .panels()
                .iter()
                .filter(|panel| panel.holds(view))
                .count(),
            1,
            "{view:?}"
        );
    }
}

#[test]
fn a_split_opens_the_picker_on_an_empty_panel_and_a_pick_moves_the_view() {
    let mut panels = Panels::default();
    panels.split_panel(BranchId(1), Direction::Across);
    assert_eq!(panels.picker(), Some(BranchId(7)));
    panels.pick(BranchId(7), View::Listing);
    assert_eq!(panels.picker(), None);
    assert_eq!(
        panels.to_string(),
        "down6(820 across5(220 0[Paths* Symbols Files] across4(740 across8(500 1[Path* Diff Graph Results] 7[Listing*]) 2[Xrefs*])) 3[Output*])"
    );
}

#[test]
fn taking_the_last_view_from_a_panel_closes_it() {
    let mut panels = Panels::default();
    panels.put(View::Xrefs, BranchId(0));
    assert_eq!(
        panels.to_string(),
        "down6(820 across5(220 0[Paths Symbols Files Xrefs*] 1[Path* Diff Graph Listing Results]) 3[Output*])"
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
    panels.drop_view(View::Output, target);
    assert_eq!(
        panels.to_string(),
        "across5(220 down8(500 0[Paths* Symbols Files] 7[Output*]) across4(740 1[Path* Diff Graph Listing Results] 2[Xrefs*]))"
    );
    let alone = DropTarget {
        panel: BranchId(7),
        zone: Zone::Edge(Edge::Left),
        band: rect(0, 0, 1, 1),
    };
    let before = panels.to_string();
    panels.drop_view(View::Output, alone);
    assert_eq!(panels.to_string(), before);
}

#[test]
fn closing_a_panel_hands_its_place_to_its_sibling() {
    let mut panels = Panels::default();
    panels.close(BranchId(3));
    panels.close(BranchId(2));
    assert_eq!(
        panels.to_string(),
        "across5(220 0[Paths* Symbols Files] 1[Path* Diff Graph Listing Results])"
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
    panels.reveal(Tab::Listing, Ticket::default());
    assert_eq!(panels.holder(View::Listing), None);
    let mut asked = Ticket::default();
    asked = asked.next();
    panels.reveal(Tab::Listing, asked);
    assert_eq!(panels.holder(View::Listing), Some(BranchId(0)));
    assert!(panels.is_shown(View::Listing));
    panels.activate(View::Paths);
    panels.reveal(Tab::Listing, asked);
    assert!(panels.is_shown(View::Paths));
    panels.reveal(Tab::Listing, asked.next());
    assert!(panels.is_shown(View::Listing));
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
    assert_eq!(divided.sash, rect(0, 794, 1600, 8));
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
    panels.take_hold(View::Paths, at(10, 10));
    panels.drag_to(at(14, 14));
    assert!(!panels.grab().unwrap().is_moving());
    panels.drag_to(at(15, 14));
    assert!(panels.grab().unwrap().is_moving());
}

#[test]
fn views_are_found_by_name_in_any_case() {
    assert_eq!(View::named(&Label::new("listing")), Some(View::Listing));
    assert_eq!(View::named(&Label::new("XREFS")), Some(View::Xrefs));
    assert_eq!(View::named(&Label::new("nothing")), None);
}
