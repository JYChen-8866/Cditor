use super::*;

#[test]
fn right_panel_options_match_upstream_widget() {
    assert_eq!(
        PATH_STYLES.map(|(_, label)| label),
        ["Direct", "Flowing", "Angular"]
    );
    assert_eq!(
        STROKE_STYLES.map(|(_, label)| label),
        ["Solid", "Dashed", "Dotted"]
    );
    assert_eq!(
        SLOPPINESS.map(|(_, label)| label),
        ["Architect", "Artist", "Cartoonist", "Drunk"]
    );
    assert_eq!(
        FILL_PATTERNS.map(|(_, label)| label),
        ["Solid", "Hatch", "Cross", "Dots"]
    );
}
