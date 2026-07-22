use crate::{
    EditorMetrics, GuiTheme, ThemeId, ThemeVersion, Typography, default_theme::default_palette,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedTheme {
    pub id: ThemeId,
    pub version: ThemeVersion,
    pub colors: GuiTheme,
    pub typography: Typography,
    pub metrics: EditorMetrics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeResolver {
    version: ThemeVersion,
}

impl Default for ThemeResolver {
    fn default() -> Self {
        Self {
            version: ThemeVersion(1),
        }
    }
}

impl ThemeResolver {
    pub const fn version(&self) -> ThemeVersion {
        self.version
    }

    pub fn invalidate(&mut self) -> ThemeVersion {
        self.version.0 = self.version.0.saturating_add(1);
        self.version
    }

    pub fn resolve(self, preference: ThemePreference, system_is_dark: bool) -> ResolvedTheme {
        let id = match preference {
            ThemePreference::System if system_is_dark => ThemeId::CditorDark,
            ThemePreference::System | ThemePreference::Light => ThemeId::CditorLight,
            ThemePreference::Dark => ThemeId::CditorDark,
        };
        ResolvedTheme {
            id,
            version: self.version,
            colors: default_palette(id),
            typography: Typography::notion_like(),
            metrics: EditorMetrics::notion_like(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_preference_tracks_platform_while_explicit_choice_wins() {
        let resolver = ThemeResolver::default();
        assert_eq!(
            resolver.resolve(ThemePreference::System, true).id,
            ThemeId::CditorDark
        );
        assert_eq!(
            resolver.resolve(ThemePreference::Light, true).id,
            ThemeId::CditorLight
        );
    }

    #[test]
    fn invalidation_is_monotonic_and_preserved_in_resolution() {
        let mut resolver = ThemeResolver::default();
        let version = resolver.invalidate();
        assert_eq!(version, ThemeVersion(2));
        assert_eq!(
            resolver.resolve(ThemePreference::Light, false).version,
            version
        );
    }
}
