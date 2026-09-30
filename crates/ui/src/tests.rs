use std::rc::Rc;

use strum::VariantArray;

use crate::{
    Button, Canvas, Cell, Color, Command, Coordinate, Count, Extent, FontSize, Glyph, Grid, Icon,
    Id, Input, Key, Kind, Label, Layout, Measure, Mods, Point, Press, Px, Rect, Run, Scale,
    Scrollbar, Sides, Style, Text, Ui, Vector, Wrap,
};

struct Cells;

impl Measure for Cells {
    fn cell(&mut self, _size: FontSize) -> Extent {
        Extent::new(Px::new(8), Px::new(16))
    }
}

fn px(value: i32) -> Px {
    Px::new(value)
}

fn rect(left: i32, top: i32, width: i32, height: i32) -> Rect {
    Rect::new(px(left), px(top), px(width), px(height))
}

fn text(content: &str) -> Kind {
    Kind::Text(Text {
        runs: vec![Run::new(content, Color::rgba(255, 255, 255, 255))],
        size: FontSize::new(14),
        wrap: Wrap::None,
    })
}

fn input(width: i32, height: i32) -> Input {
    Input {
        size: Extent::new(px(width), px(height)),
        ..Input::default()
    }
}

fn legacy_id(name: &str) -> u64 {
    legacy_with(0xcbf2_9ce4_8422_2325, name)
}

fn legacy_with(base: u64, name: &str) -> u64 {
    let mut hash = base;
    for byte in name.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

fn legacy_nth(base: u64, number: usize) -> u64 {
    (base
        ^ u64::try_from(number)
            .unwrap()
            .wrapping_mul(0x9e37_79b9_7f4a_7c15))
    .wrapping_mul(0x0100_0000_01b3)
}

#[test]
fn row_fits_and_grows() {
    let mut ui = Ui::default();
    ui.begin(&input(200, 100));
    ui.open(
        Kind::None,
        Layout::row().grow().padding(px(2)).gap(px(4)),
        Style::default(),
        None,
    );
    ui.leaf(
        text("abc"),
        Layout::row(),
        Style::default(),
        Some(Id::new("one")),
    );
    ui.leaf(
        Kind::None,
        Layout::row().grow(),
        Style::default(),
        Some(Id::new("two")),
    );
    ui.leaf(
        text("de"),
        Layout::row(),
        Style::default(),
        Some(Id::new("three")),
    );
    ui.close();
    ui.end(&mut Cells);
    let placed = |name| ui.placement(Id::new(name)).unwrap().rect;
    assert_eq!(placed("one"), rect(2, 2, 24, 16));
    assert_eq!(placed("two"), rect(30, 2, 200 - 4 - 24 - 16 - 8, 96));
    assert_eq!(placed("three").left, px(200 - 2 - 16));
}

#[test]
fn wrapped_text_takes_lines() {
    let mut ui = Ui::default();
    ui.begin(&input(100, 100));
    ui.open(Kind::None, Layout::column().grow(), Style::default(), None);
    ui.leaf(
        Kind::Text(Text {
            runs: vec![Run::new(
                "one two three four",
                Color::rgba(255, 255, 255, 255),
            )],
            size: FontSize::new(14),
            wrap: Wrap::Words,
        }),
        Layout::row().grow_width(),
        Style::default(),
        Some(Id::new("wrapped")),
    );
    ui.close();
    ui.end(&mut Cells);
    assert_eq!(
        Label::new("one two three four").wrap(12),
        [Label::new("one two"), Label::new("three four")]
    );
    assert_eq!(
        ui.placement(Id::new("wrapped")).unwrap().rect.height,
        px(32)
    );
}

#[test]
fn hit_test_is_one_frame_late() {
    let mut ui = Ui::default();
    let mut frame = input(100, 100);
    frame.pointer.mouse = Point::new(px(10), px(10));
    let id = Id::new("box");
    ui.begin(&frame);
    assert!(
        !ui.leaf(
            Kind::None,
            Layout::row().width(px(50)).height(px(50)),
            Style::default(),
            Some(id)
        )
        .hovered()
    );
    ui.end(&mut Cells);
    frame.pointer.pressed.insert(Button::Left);
    ui.begin(&frame);
    let interaction = ui.leaf(
        Kind::None,
        Layout::row().width(px(50)).height(px(50)),
        Style::default(),
        Some(id),
    );
    assert!(interaction.hovered() && interaction.clicked());
}

#[test]
fn ids_hash_like_the_legacy_names() {
    assert_eq!(Id::new("document").get(), legacy_id("document"));
    assert_eq!(
        Id::new("tab").with("Graph").get(),
        legacy_with(legacy_id("tab"), "Graph")
    );
    assert_eq!(
        Id::new("step").nth(7).get(),
        legacy_nth(legacy_id("step"), 7)
    );
    assert_eq!(Id::from_name("step/7"), Id::new("step").nth(7));
    assert_eq!(
        Id::from_name("left@Symbols"),
        Id::new("left").with("Symbols")
    );
    assert_eq!(Id::from_name("paths"), Id::new("paths"));
}

#[test]
fn smallest_hovered_element_is_hot() {
    let mut ui = Ui::default();
    let mut frame = input(100, 100);
    frame.pointer.mouse = Point::new(px(5), px(5));
    for _ in 0..2 {
        ui.begin(&frame);
        ui.open(
            Kind::None,
            Layout::column().grow(),
            Style::default(),
            Some(Id::new("outer")),
        );
        ui.leaf(
            Kind::None,
            Layout::row().width(px(10)).height(px(10)),
            Style::default(),
            Some(Id::new("inner")),
        );
        ui.close();
        ui.end(&mut Cells);
    }
    assert_eq!(ui.hot(), Some(Id::new("inner")));
}

#[test]
fn centered_cross_axis_and_floating() {
    let mut ui = Ui::default();
    ui.begin(&input(100, 100));
    ui.open(
        Kind::None,
        Layout::row()
            .width(px(60))
            .height(px(40))
            .cross(crate::Align::Center),
        Style::default(),
        None,
    );
    ui.leaf(
        Kind::None,
        Layout::row().width(px(10)).height(px(10)),
        Style::default(),
        Some(Id::new("centered")),
    );
    ui.leaf(
        Kind::None,
        Layout::row()
            .width(px(5))
            .height(px(5))
            .floating(Point::new(px(70), px(80))),
        Style::default(),
        Some(Id::new("floating")),
    );
    ui.close();
    ui.end(&mut Cells);
    assert_eq!(
        ui.placement(Id::new("centered")).unwrap().rect,
        rect(0, 15, 10, 10)
    );
    let floating = ui.placement(Id::new("floating")).unwrap();
    assert_eq!(floating.rect, rect(70, 80, 5, 5));
    assert_eq!(floating.clip, rect(0, 0, 100, 100));
}

#[test]
fn scrolled_column_clips_children_and_reports_content() {
    let mut ui = Ui::default();
    ui.begin(&input(100, 100));
    ui.open(
        Kind::None,
        Layout::column()
            .width(px(50))
            .height(px(30))
            .scroll(Point::new(px(0), px(12))),
        Style::default(),
        Some(Id::new("list")),
    );
    for row in 0..4 {
        ui.leaf(
            Kind::None,
            Layout::row().width(px(50)).height(px(20)),
            Style::default(),
            Some(Id::new("row").nth(row)),
        );
    }
    ui.close();
    ui.end(&mut Cells);
    let list = ui.placement(Id::new("list")).unwrap();
    assert_eq!(list.content, Extent::new(px(50), px(80)));
    let second = ui.placement(Id::new("row").nth(1)).unwrap();
    assert_eq!(second.rect, rect(0, 8, 50, 20));
    assert_eq!(second.clip, rect(0, 0, 50, 30));
    assert_eq!(second.visible_center(), Some(Point::new(px(25), px(18))));
    let scrolled_above = ui.placement(Id::new("row").nth(0)).unwrap();
    assert_eq!(scrolled_above.visible_center(), None);
    let below_the_fold = ui.placement(Id::new("row").nth(3)).unwrap();
    assert_eq!(below_the_fold.visible_center(), None);
    let drawn = ui.draw(&mut Cells, Color::rgba(1, 2, 3, 255));
    let fills: Vec<Rect> = drawn
        .commands()
        .filter_map(|command| match command {
            Command::Fill { rect, .. } => Some(*rect),
            _ => None,
        })
        .collect();
    assert_eq!(fills, [rect(42, 0, 8, 30), rect(43, 3, 6, 18)]);
}

#[test]
fn scrollbar_matches_legacy_math() {
    let bar = Scrollbar::of(rect(0, 0, 100, 50), px(200), px(75)).unwrap();
    assert_eq!(bar.track, rect(92, 0, 8, 50));
    assert_eq!(bar.thumb, rect(92, 15, 8, 20));
    assert!(Scrollbar::of(rect(0, 0, 100, 50), px(50), px(0)).is_none());
}

#[test]
fn wheel_scrolls_and_clamps() {
    let mut ui = Ui::default();
    let mut frame = input(100, 100);
    frame.pointer.mouse = Point::new(px(10), px(10));
    let id = Id::new("list");
    let mut offset = px(0);
    for _ in 0..2 {
        ui.begin(&frame);
        ui.scroll_by_wheel(id, &mut offset);
        ui.open(
            Kind::None,
            Layout::column()
                .width(px(50))
                .height(px(30))
                .scroll(Point::new(px(0), offset)),
            Style::default(),
            Some(id),
        );
        ui.leaf(
            Kind::None,
            Layout::row().width(px(50)).height(px(100)),
            Style::default(),
            None,
        );
        ui.close();
        ui.end(&mut Cells);
        frame.pointer.wheel = Vector::new(Coordinate::new(0.0), Coordinate::new(-120.0));
    }
    assert_eq!(offset, px(70));
}

#[test]
fn borders_and_text_become_commands() {
    let mut ui = Ui::default();
    ui.begin(&input(100, 100));
    ui.leaf(
        text("hi"),
        Layout::row().padding(px(1)),
        Style::background(Color::rgba(9, 9, 9, 255))
            .border(Sides::LEFT.with(Sides::BOTTOM), Color::rgba(5, 5, 5, 255)),
        None,
    );
    ui.end(&mut Cells);
    let drawn = ui.draw(&mut Cells, Color::rgba(1, 1, 1, 255));
    let kinds: Vec<String> = drawn
        .commands()
        .map(|command| match command {
            Command::Clip(clip) => format!("clip {clip:?}"),
            Command::Fill { rect, .. } => format!("fill {rect:?}"),
            Command::Text { at, text, .. } => format!("text {at:?} {}", text.as_str()),
            other => format!("{other:?}"),
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "clip Rect { x: 0, y: 0, w: 100, h: 100 }",
            "fill Rect { x: 0, y: 0, w: 18, h: 18 }",
            "text (1, 1) hi",
            "fill Rect { x: 0, y: 0, w: 1, h: 18 }",
            "fill Rect { x: 0, y: 17, w: 18, h: 1 }",
            "clip Rect { x: 0, y: 0, w: 100, h: 100 }",
        ]
    );
}

#[test]
fn canvas_text_stops_at_the_clip() {
    let mut cells = Cells;
    let mut canvas = Canvas::new(Extent::new(px(100), px(100)), &mut cells);
    canvas.push_clip(rect(0, 0, 20, 20));
    let pen = canvas.text(
        Point::new(px(0), px(0)),
        FontSize::new(14),
        "abcdef",
        Color::rgba(1, 1, 1, 255),
    );
    assert_eq!(pen, px(24));
    canvas.pop_clip();
    canvas.pop_clip();
    assert_eq!(canvas.clip(), rect(0, 0, 100, 100));
    let grid = Rc::new(Grid::new(Count::new(2), Count::new(1)));
    canvas.grid(Point::default(), FontSize::new(14), &grid);
    canvas.rect(rect(0, 0, 0, 5), Color::rgba(1, 1, 1, 255));
    canvas.rect(rect(0, 0, 5, 5), Color::rgba(1, 1, 1, 0));
    assert_eq!(canvas.finish().commands().count(), 4);
}

#[test]
fn grid_rows_hold_cells() {
    let mut grid = Grid::new(Count::new(3), Count::new(2));
    let cell = Cell {
        glyph: Glyph::new('x'),
        color: Color::rgba(1, 2, 3, 4),
    };
    grid.set(Count::new(1), Count::new(1), cell);
    grid.set(Count::new(5), Count::new(1), cell);
    assert_eq!(
        grid.row(Count::new(1)).collect::<Vec<_>>(),
        [None, Some(cell), None]
    );
    assert_eq!(grid.row(Count::new(0)).flatten().count(), 0);
}

#[test]
fn line_and_curve_follow_the_legacy_float_math() {
    let mut cells = Cells;
    let mut canvas = Canvas::new(Extent::new(px(100), px(100)), &mut cells);
    let point = |horizontal: f32, vertical: f32| {
        Vector::new(Coordinate::new(horizontal), Coordinate::new(vertical))
    };
    canvas.line(
        point(0.0, 0.0),
        point(10.0, 0.0),
        Coordinate::new(2.0),
        Color::rgba(1, 1, 1, 255),
    );
    canvas.line(
        point(1.0, 1.0),
        point(1.0, 1.0),
        Coordinate::new(2.0),
        Color::rgba(1, 1, 1, 255),
    );
    canvas.curve(
        [
            point(0.0, 0.0),
            point(5.0, 0.0),
            point(5.0, 10.0),
            point(10.0, 10.0),
        ],
        Coordinate::new(1.0),
        Color::rgba(1, 1, 1, 255),
    );
    let drawn = canvas.finish();
    let quads: Vec<[Vector; 4]> = drawn
        .commands()
        .filter_map(|command| match command {
            Command::Quad { corners, .. } => Some(*corners),
            _ => None,
        })
        .collect();
    assert_eq!(quads.len(), 25);
    assert_eq!(
        quads[0],
        [
            point(0.0, 1.0),
            point(10.0, 1.0),
            point(10.0, -1.0),
            point(0.0, -1.0)
        ]
    );
}

#[test]
fn numbers_convert_like_casts() {
    for (value, expected) in [
        (0, 0.0_f32),
        (1, 1.0),
        (-1, -1.0),
        (16_777_217, 16_777_216.0),
        (-16_777_219, -16_777_220.0),
        (2_147_483_647, 2_147_483_648.0),
        (-2_147_483_648, -2_147_483_648.0),
        (12345, 12345.0),
    ] {
        assert_eq!(
            Coordinate::of_integer(value).get().to_bits(),
            expected.to_bits()
        );
    }
    for (value, expected) in [
        (0.0_f32, 0),
        (0.9, 0),
        (-0.9, 0),
        (1.5, 1),
        (-2.5, -2),
        (3e9, 2_147_483_647),
        (-3e9, -2_147_483_648),
        (f32::NAN, 0),
        (1234.99, 1234),
    ] {
        assert_eq!(Coordinate::new(value).truncate().get(), expected);
    }
    for (value, expected) in [
        (0.1_f64, 0.1_f32),
        (-0.1, -0.1),
        (1.0 / 3.0, 1.0 / 3.0),
        (1e30, 1e30),
        (1e-30, 1e-30),
        (123_456_789.123, 123_456_792.0),
        (0.0, 0.0),
        (-0.0, -0.0),
    ] {
        assert_eq!(
            Coordinate::narrow(value).get().to_bits(),
            expected.to_bits()
        );
    }
    assert_eq!(FontSize::scaled(14, Scale::ONE, 8), FontSize::new(14));
    assert_eq!(FontSize::scaled(14, Scale::new(0.25), 8), FontSize::new(8));
}

#[test]
fn keys_match_ctrl_and_alt_but_not_shift() {
    let mut frame = Input::default();
    frame.keys.push(Press {
        key: Key::Character(Glyph::new('s')),
        mods: Mods::CTRL.with(Mods::SHIFT),
    });
    assert!(frame.pressed_key(Key::Character(Glyph::new('s')), Mods::CTRL));
    assert!(!frame.pressed_key(Key::Character(Glyph::new('s')), Mods::NONE));
    assert_eq!(
        format!("{:?}", Mods::new(true, false, true)),
        "Mods { ctrl: true, shift: false, alt: true }"
    );
    frame.pointer.pressed.insert(Button::Back);
    frame.end_frame();
    assert!(frame.keys.is_empty() && !frame.pointer.pressed.contains(Button::Back));
}

#[test]
fn an_icon_is_two_columns_wide_and_spells_its_name() {
    let label = Label::new(format!("{} Paths", Icon::Close.glyph().get()));
    assert_eq!(label.columns(), 8);
    assert_eq!(label.spelled(), "[close] Paths");
    assert!(Icon::VARIANTS.iter().all(|icon| icon.glyph().is_icon()));
    assert!(!Glyph::new('x').is_icon());
    assert_eq!(Icon::of(Icon::SplitDown.glyph()), Some(Icon::SplitDown));
}

#[test]
fn wrapping_counts_an_icon_as_two_columns() {
    let icon = Icon::Add.glyph().get();
    let label = Label::new(format!("{icon}{icon}{icon} ab"));
    assert_eq!(
        label.wrap(6),
        [Label::new(format!("{icon}{icon}{icon}")), Label::new("ab")]
    );
}

#[test]
fn clipped_text_fits_its_rect_and_ends_in_an_ellipsis() {
    let white = Color::rgba(255, 255, 255, 255);
    let grey = Color::rgba(128, 128, 128, 255);
    let mut ui = Ui::default();
    ui.begin(&input(100, 100));
    ui.open(Kind::None, Layout::row().width(px(48)), Style::NONE, None);
    ui.leaf(
        Kind::Text(Text {
            runs: vec![Run::new("name", white), Run::new(" src/lib.rs", grey)],
            size: FontSize::new(14),
            wrap: Wrap::Clip,
        }),
        Layout::row().grow_width(),
        Style::NONE,
        None,
    );
    ui.close();
    ui.end(&mut Cells);
    let drawn = ui.draw(&mut Cells, white);
    let texts: Vec<String> = drawn
        .commands()
        .filter_map(|command| match command {
            Command::Text { at, text, .. } => Some(format!("{at:?} {}", text.as_str())),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["(0, 0) name", "(32, 0)  \u{2026}"]);
}
