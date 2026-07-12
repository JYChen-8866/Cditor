use gpui::{Hsla, rgb};
use mermaid_render::{AccentColor, MermaidTheme, text_color_for_background};

use crate::gui::GuiTheme;

fn color(value: u32) -> Hsla {
    rgb(value).into()
}

pub(super) fn build_mermaid_theme(theme: GuiTheme) -> MermaidTheme {
    let git_branch_colors = [
        color(0x2383e2),
        color(0x0f7b6c),
        color(0xd9730d),
        color(0x9b51e0),
        color(0xeb5757),
        color(0x2f80ed),
        color(0x27ae60),
        color(0xf2c94c),
    ];
    let git_branch_label_colors = git_branch_colors.map(text_color_for_background);
    let accent_colors = [
        (0x0b6e99, 0xddebf1),
        (0x0f7b6c, 0xdbeddb),
        (0x9a6700, 0xfdecc8),
        (0x6940a5, 0xe8deee),
        (0xa61b1b, 0xffe2dd),
        (0x1f6f8b, 0xd3e5ef),
    ]
    .into_iter()
    .map(|(foreground, background)| AccentColor {
        foreground: color(foreground),
        background: color(background),
    })
    .collect();

    MermaidTheme {
        dark_mode: false,
        font_family: "Inter, ui-sans-serif, system-ui, -apple-system, sans-serif".to_owned(),
        background: color(theme.code_background),
        primary_color: color(theme.code_background),
        primary_text_color: color(theme.text),
        primary_border_color: color(theme.strong_border),
        secondary_color: color(theme.hover_surface),
        tertiary_color: color(theme.page),
        line_color: color(theme.muted),
        text_color: color(theme.text),
        edge_label_background: color(theme.code_background),
        cluster_background: color(theme.code_background),
        cluster_border: color(theme.border),
        note_background: color(0xfff7d6),
        note_border: color(0xe8c547),
        actor_background: color(theme.code_background),
        actor_border: color(theme.strong_border),
        activation_background: color(theme.hover_surface),
        activation_border: color(theme.strong_border),
        git_branch_colors,
        git_branch_label_colors,
        er_attr_bg_odd: color(theme.code_background),
        er_attr_bg_even: color(theme.code_background),
        error_color: color(theme.danger),
        warning_color: color(0xd9730d),
        accent_colors,
    }
}

#[cfg(test)]
mod tests {
    use gpui::Rgba;

    use super::*;

    #[test]
    fn cditor_theme_maps_editor_text_and_background() {
        let gui = GuiTheme::light();
        let mermaid = build_mermaid_theme(gui);

        assert_eq!(
            Rgba::from(mermaid.background),
            Rgba::from(color(gui.code_background))
        );
        assert_eq!(Rgba::from(mermaid.text_color), Rgba::from(color(gui.text)));
        assert_eq!(mermaid.git_branch_colors.len(), 8);
        assert!(!mermaid.accent_colors.is_empty());
    }

    #[test]
    fn zed_renderer_produces_raster_safe_svg() {
        let theme = build_mermaid_theme(GuiTheme::light());
        let source = "flowchart LR\n  A[Long source label] --> B[Rendered diagram]";
        let svg = mermaid_render::render_to_svg(source, &theme).expect("mermaid should render");

        assert!(svg.contains("<svg"));
        assert!(!svg.contains("<foreignObject"));
        assert!(!svg.contains("<foreignobject"));
    }
}
