use std::{
    env, fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;

#[derive(Deserialize)]
struct SourceConfig {
    document: SourceDocument,
}

#[derive(Deserialize)]
struct SourceDocument {
    typography: SourceTypography,
    table: SourceTable,
}

#[derive(Deserialize)]
struct SourceTypography {
    fonts: SourceFonts,
    styles: SourceStyles,
}

#[derive(Deserialize)]
struct SourceFonts {
    body: SourceBodyFont,
    code: SourcePlatformFont,
    ui: SourcePlatformFont,
}

#[derive(Deserialize)]
struct SourceBodyFont {
    family: String,
    regular_asset: String,
    medium_asset: String,
    semibold_asset: String,
    bold_asset: String,
}

#[derive(Deserialize)]
struct SourcePlatformFont {
    macos: String,
    windows: String,
    linux: String,
}

#[derive(Deserialize)]
struct SourceStyles {
    body: SourceTextStyle,
    heading_1: SourceTextStyle,
    heading_2: SourceTextStyle,
    heading_3: SourceTextStyle,
    footnote: SourceTextStyle,
    table_cell: SourceTextStyle,
    table_header: SourceTextStyle,
    code: SourceTextStyle,
    ui: SourceTextStyle,
}

#[derive(Deserialize)]
struct SourceTextStyle {
    size_px: f32,
    line_height_px: f32,
    weight: u16,
}

#[derive(Deserialize)]
struct SourceTable {
    default_row_height_px: f32,
    cell_padding_x_px: f32,
    cell_padding_y_px: f32,
}

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let workspace_root = manifest_dir.join("../..");
    let config_path = workspace_root.join("config/app.toml");
    println!("cargo:rerun-if-changed={}", config_path.display());

    let source = fs::read_to_string(&config_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", config_path.display()));
    let config: SourceConfig = toml::from_str(&source)
        .unwrap_or_else(|error| panic!("invalid {}: {error}", config_path.display()));
    validate(&config);

    let generated = render(&config, &workspace_root);
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("app_config.rs");
    fs::write(output, generated).expect("write generated app config");
}

fn validate(config: &SourceConfig) {
    let typography = &config.document.typography;
    assert!(
        !typography.fonts.body.family.trim().is_empty(),
        "body font family must not be empty"
    );
    for (name, style) in [
        ("body", &typography.styles.body),
        ("heading_1", &typography.styles.heading_1),
        ("heading_2", &typography.styles.heading_2),
        ("heading_3", &typography.styles.heading_3),
        ("footnote", &typography.styles.footnote),
        ("table_cell", &typography.styles.table_cell),
        ("table_header", &typography.styles.table_header),
        ("code", &typography.styles.code),
        ("ui", &typography.styles.ui),
    ] {
        assert!(
            style.size_px.is_finite() && style.size_px > 0.0,
            "{name}.size_px must be positive"
        );
        assert!(
            style.line_height_px.is_finite() && style.line_height_px >= style.size_px,
            "{name}.line_height_px must be at least size_px"
        );
        assert!(
            (1..=1000).contains(&style.weight),
            "{name}.weight must be in 1..=1000"
        );
    }
    let table = &config.document.table;
    assert!(table.default_row_height_px > 0.0);
    assert!(table.cell_padding_x_px >= 0.0);
    assert!(table.cell_padding_y_px >= 0.0);
}

fn render(config: &SourceConfig, workspace_root: &Path) -> String {
    let document = &config.document;
    let typography = &document.typography;
    let fonts = &typography.fonts;
    let styles = &typography.styles;
    let regular = asset_path(workspace_root, &fonts.body.regular_asset);
    let medium = asset_path(workspace_root, &fonts.body.medium_asset);
    let semibold = asset_path(workspace_root, &fonts.body.semibold_asset);
    let bold = asset_path(workspace_root, &fonts.body.bold_asset);
    for path in [&regular, &medium, &semibold, &bold] {
        println!("cargo:rerun-if-changed={}", path.display());
        assert!(
            path.is_file(),
            "configured font asset does not exist: {}",
            path.display()
        );
    }

    format!(
        "pub const APP_CONFIG: AppConfig = AppConfig {{\n    document: DocumentConfig {{\n        typography: DocumentTypographyConfig {{\n            fonts: DocumentFontsConfig {{\n                body: BodyFontConfig {{ family: {family:?}, regular: include_bytes!({regular:?}), medium: include_bytes!({medium:?}), semibold: include_bytes!({semibold:?}), bold: include_bytes!({bold:?}) }},\n                code: PlatformFontConfig {{ macos: {code_macos:?}, windows: {code_windows:?}, linux: {code_linux:?} }},\n                ui: PlatformFontConfig {{ macos: {ui_macos:?}, windows: {ui_windows:?}, linux: {ui_linux:?} }},\n            }},\n            styles: DocumentTextStylesConfig {{ body: {body}, heading_1: {h1}, heading_2: {h2}, heading_3: {h3}, footnote: {footnote}, table_cell: {table_cell}, table_header: {table_header}, code: {code}, ui: {ui} }},\n        }},\n        table: TableConfig {{ default_row_height_px: {row_height:?}, cell_padding_x_px: {padding_x:?}, cell_padding_y_px: {padding_y:?} }},\n    }},\n}};\n",
        family = fonts.body.family,
        regular = regular.display().to_string(),
        medium = medium.display().to_string(),
        semibold = semibold.display().to_string(),
        bold = bold.display().to_string(),
        code_macos = fonts.code.macos,
        code_windows = fonts.code.windows,
        code_linux = fonts.code.linux,
        ui_macos = fonts.ui.macos,
        ui_windows = fonts.ui.windows,
        ui_linux = fonts.ui.linux,
        body = style(&styles.body),
        h1 = style(&styles.heading_1),
        h2 = style(&styles.heading_2),
        h3 = style(&styles.heading_3),
        footnote = style(&styles.footnote),
        table_cell = style(&styles.table_cell),
        table_header = style(&styles.table_header),
        code = style(&styles.code),
        ui = style(&styles.ui),
        row_height = document.table.default_row_height_px,
        padding_x = document.table.cell_padding_x_px,
        padding_y = document.table.cell_padding_y_px,
    )
}

fn style(style: &SourceTextStyle) -> String {
    format!(
        "TextStyleConfig {{ size_px: {:?}, line_height_px: {:?}, weight: {} }}",
        style.size_px, style.line_height_px, style.weight
    )
}

fn asset_path(workspace_root: &Path, configured: &str) -> PathBuf {
    let path = workspace_root.join(configured);
    path.canonicalize()
        .unwrap_or_else(|error| panic!("invalid configured asset {}: {error}", path.display()))
}
