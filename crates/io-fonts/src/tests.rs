use std::path::PathBuf;

use super::*;

fn asset(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets")
        .join(name)
}

#[test]
fn a_monospaced_face_is_listed_by_its_family_and_an_icon_font_is_not() {
    let fonts = Fonts::of_files(&[asset("codicon.ttf"), asset("Hack-Regular.ttf")]);
    let families = fonts
        .families()
        .map(FontFamily::as_str)
        .collect::<Vec<&str>>();
    assert_eq!(families, ["Hack"]);
}

#[test]
fn reading_a_family_returns_its_whole_file_and_face() {
    let fonts = Fonts::of_files(&[asset("Hack-Regular.ttf")]);
    let file = fonts.read(&FontFamily::new("Hack").unwrap()).unwrap();
    assert_eq!(
        file.bytes().as_slice(),
        fs::read(asset("Hack-Regular.ttf")).unwrap()
    );
    assert_eq!(file.index(), FaceIndex::new(0));
    assert_eq!(fonts.read(&FontFamily::new("Menlo").unwrap()), None);
}
