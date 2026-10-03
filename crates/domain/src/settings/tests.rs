use super::*;

#[test]
fn a_font_size_outside_eight_to_thirty_two_is_refused() {
    assert_eq!(BaseFontSize::new(7), None);
    assert_eq!(BaseFontSize::new(33), None);
    assert_eq!(BaseFontSize::new(8).map(BaseFontSize::get), Some(8));
    assert_eq!(BaseFontSize::new(32).map(BaseFontSize::get), Some(32));
}

#[test]
fn larger_and_smaller_stop_at_the_bounds() {
    assert_eq!(BaseFontSize::LARGEST.larger(), BaseFontSize::LARGEST);
    assert_eq!(BaseFontSize::SMALLEST.smaller(), BaseFontSize::SMALLEST);
    assert_eq!(BaseFontSize::default().larger().get(), 15);
    assert_eq!(BaseFontSize::default().smaller().get(), 13);
}

#[test]
fn a_family_name_is_trimmed_and_never_empty_or_multi_line() {
    assert_eq!(FontFamily::new("  "), None);
    assert_eq!(FontFamily::new("Menlo\nBold"), None);
    assert_eq!(
        FontFamily::new(" JetBrains Mono ").map(|family| family.to_string()),
        Some("JetBrains Mono".to_owned())
    );
}
