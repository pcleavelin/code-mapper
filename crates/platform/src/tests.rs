use wgpu::TextureFormat;

use crate::atlas::{Atlas, Bitmap, Corner, Texel};
use crate::gpu::bytes_of;
use crate::present::Pixels;
use crate::script::ScriptLine;
use ui::{Point, Px};

fn texts(lines: &[ScriptLine]) -> Vec<&str> {
    lines.iter().map(ScriptLine::as_str).collect()
}

#[test]
fn click_expands_into_press_and_release() {
    let line = ScriptLine::new("dblclick 10 20 ctrl");
    assert_eq!(
        texts(&line.click()),
        ["mouse 10 20", "down ctrl twice", "wait 1", "up", "wait 1"]
    );
    assert_eq!(
        texts(&ScriptLine::new("click 1 2").click()),
        ["mouse 1 2", "down ", "wait 1", "up", "wait 1"]
    );
}

#[test]
fn aimed_gestures_use_the_element_center() {
    let at = Point::new(Px::new(30), Px::new(40));
    assert_eq!(
        ScriptLine::new("hover-id row/3").aimed(at).as_str(),
        "mouse 30 40 "
    );
    assert_eq!(
        ScriptLine::new("click-id tab@Graph alt").aimed(at).as_str(),
        "click 30 40 alt"
    );
}

#[test]
fn drag_moves_in_eight_steps() {
    let lines = ScriptLine::new("drag 0 0 80 16").drag();
    assert_eq!(lines.len(), 3 + 16 + 2);
    assert_eq!(lines[3].as_str(), "mouse 10 2");
    assert_eq!(lines[17].as_str(), "mouse 80 16");
    assert_eq!(lines[20].as_str(), "wait 1");
}

#[test]
fn numbers_and_mods_parse_like_the_legacy_runner() {
    let line = ScriptLine::new("wheel -120 ctrl shift");
    assert_eq!(line.number(1), -120);
    assert_eq!(line.number(5), 0);
    let mods = line.mods_from(2);
    assert!(mods.ctrl() && mods.shift() && !mods.alt());
    assert_eq!(line.rest(1), "-120 ctrl shift");
}

#[test]
fn atlas_packs_rows_after_the_white_block() {
    let mut atlas = Atlas::new(Texel::of_count(16));
    let first = atlas
        .allocate(Texel::of_count(4), Texel::of_count(4))
        .unwrap();
    assert_eq!(
        first,
        Corner {
            left: Texel::of_count(5),
            top: Texel::of_count(0)
        }
    );
    let second = atlas
        .allocate(Texel::of_count(8), Texel::of_count(2))
        .unwrap();
    assert_eq!(second.top, Texel::of_count(5));
    assert_eq!(second.left, Texel::of_count(0));
    assert!(
        atlas
            .allocate(Texel::of_count(3), Texel::of_count(20))
            .is_none()
    );
    atlas.blit(first, Texel::of_count(2), &Bitmap::new(vec![7, 8, 9, 10]));
    let pixels = atlas.pixels().as_slice();
    assert_eq!(&pixels[0..6], &[255, 255, 255, 255, 0, 7]);
    assert_eq!(&pixels[16 + 5..16 + 7], &[9, 10]);
}

#[test]
fn read_back_rows_drop_padding_and_swap_blue() {
    let data = [1, 2, 3, 4, 9, 9, 5, 6, 7, 8, 9, 9];
    let blue_first = Pixels::from_rows(&data, 6, 4, 2, TextureFormat::Bgra8Unorm);
    assert_eq!(blue_first.as_bytes(), [3, 2, 1, 255, 7, 6, 5, 255]);
    let red_first = Pixels::from_rows(&data, 6, 4, 2, TextureFormat::Rgba8Unorm);
    assert_eq!(red_first.as_bytes(), [1, 2, 3, 255, 5, 6, 7, 255]);
}

#[test]
fn bytes_of_reads_plain_values() {
    let values = [1_u32, 2];
    assert_eq!(bytes_of(&values).as_slice().len(), 8);
}
