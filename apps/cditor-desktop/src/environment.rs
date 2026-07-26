use std::{env, ffi::OsString, fmt, path::PathBuf};

use cditor_sdk::Cditor;

use crate::CditorStorageExt;

const DEFAULT_DOCUMENT_ID: u64 = 1;
const DEFAULT_PAYLOAD_WINDOW_SIZE: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
enum DesktopBackendConfig {
    Demo,
    LargeDemo,
    Sqlite(PathBuf),
    Postgres(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DesktopLaunchConfig {
    backend: DesktopBackendConfig,
    document_id: u64,
    workspace_id: Option<u64>,
    readonly: bool,
    debug_overlay: bool,
    payload_window_size: usize,
}

impl DesktopLaunchConfig {
    fn from_lookup(
        mut lookup: impl FnMut(&str) -> Option<OsString>,
    ) -> Result<Self, DesktopEnvironmentError> {
        let sqlite_path = non_empty_os(lookup("CDITOR_SQLITE_PATH"));
        let database_url = optional_unicode(
            "CDITOR_DATABASE_URL",
            non_empty_os(lookup("CDITOR_DATABASE_URL")),
        )?;
        let small_demo = parse_bool("CDITOR_SMALL_DEMO", lookup("CDITOR_SMALL_DEMO"))?;
        let large_demo = parse_bool("CDITOR_LARGE_DEMO", lookup("CDITOR_LARGE_DEMO"))?;

        if sqlite_path.is_some() && database_url.is_some() {
            return Err(DesktopEnvironmentError::new(
                "CDITOR_SQLITE_PATH and CDITOR_DATABASE_URL cannot both be set",
            ));
        }
        if small_demo && large_demo {
            return Err(DesktopEnvironmentError::new(
                "CDITOR_SMALL_DEMO and CDITOR_LARGE_DEMO cannot both be enabled",
            ));
        }
        if (sqlite_path.is_some() || database_url.is_some()) && (small_demo || large_demo) {
            return Err(DesktopEnvironmentError::new(
                "database configuration cannot be combined with a demo mode",
            ));
        }

        let backend = if let Some(path) = sqlite_path {
            DesktopBackendConfig::Sqlite(PathBuf::from(path))
        } else if let Some(url) = database_url {
            DesktopBackendConfig::Postgres(url)
        } else if large_demo {
            DesktopBackendConfig::LargeDemo
        } else {
            DesktopBackendConfig::Demo
        };

        Ok(Self {
            backend,
            document_id: parse_integer("CDITOR_DOCUMENT_ID", lookup("CDITOR_DOCUMENT_ID"))?
                .unwrap_or(DEFAULT_DOCUMENT_ID),
            workspace_id: parse_integer("CDITOR_WORKSPACE_ID", lookup("CDITOR_WORKSPACE_ID"))?,
            readonly: parse_bool("CDITOR_READONLY", lookup("CDITOR_READONLY"))?,
            debug_overlay: parse_bool("CDITOR_DEBUG_OVERLAY", lookup("CDITOR_DEBUG_OVERLAY"))?,
            payload_window_size: parse_integer(
                "CDITOR_PAYLOAD_WINDOW_SIZE",
                lookup("CDITOR_PAYLOAD_WINDOW_SIZE"),
            )?
            .unwrap_or(DEFAULT_PAYLOAD_WINDOW_SIZE),
        })
    }

    fn into_cditor(self) -> Cditor {
        let mut cditor = match self.backend {
            DesktopBackendConfig::Demo => Cditor::new().demo(),
            DesktopBackendConfig::LargeDemo => Cditor::new().large_demo(),
            DesktopBackendConfig::Sqlite(path) => Cditor::new()
                .with_document_id(self.document_id)
                .with_sqlite_path(path),
            DesktopBackendConfig::Postgres(url) => Cditor::new()
                .with_document_id(self.document_id)
                .with_postgres_url(url),
        };
        if let Some(workspace_id) = self.workspace_id {
            cditor = cditor.with_workspace_id(workspace_id);
        }
        cditor
            .with_readonly(self.readonly)
            .with_debug_overlay(self.debug_overlay)
            .with_payload_window_size(self.payload_window_size)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopEnvironmentError {
    message: String,
}

impl DesktopEnvironmentError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for DesktopEnvironmentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for DesktopEnvironmentError {}

pub fn cditor_from_env() -> Result<Cditor, DesktopEnvironmentError> {
    DesktopLaunchConfig::from_lookup(|name| env::var_os(name)).map(DesktopLaunchConfig::into_cditor)
}

fn non_empty_os(value: Option<OsString>) -> Option<OsString> {
    value.filter(|value| !value.is_empty())
}

fn optional_unicode(
    name: &str,
    value: Option<OsString>,
) -> Result<Option<String>, DesktopEnvironmentError> {
    value
        .map(|value| {
            value.into_string().map_err(|_| {
                DesktopEnvironmentError::new(format!("{name} must contain valid UTF-8"))
            })
        })
        .transpose()
}

fn parse_bool(name: &str, value: Option<OsString>) -> Result<bool, DesktopEnvironmentError> {
    let Some(value) = optional_unicode(name, non_empty_os(value))? else {
        return Ok(false);
    };
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(DesktopEnvironmentError::new(format!(
            "{name} must be one of 1/true/yes/on or 0/false/no/off"
        ))),
    }
}

fn parse_integer<T>(
    name: &str,
    value: Option<OsString>,
) -> Result<Option<T>, DesktopEnvironmentError>
where
    T: std::str::FromStr,
{
    let Some(value) = optional_unicode(name, non_empty_os(value))? else {
        return Ok(None);
    };
    value
        .parse()
        .map(Some)
        .map_err(|_| DesktopEnvironmentError::new(format!("{name} must be an unsigned integer")))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use cditor_sdk::options::CditorBackend;

    use super::*;

    fn config(entries: &[(&str, &str)]) -> Result<DesktopLaunchConfig, DesktopEnvironmentError> {
        let values = entries
            .iter()
            .map(|(name, value)| ((*name).to_owned(), OsString::from(value)))
            .collect::<HashMap<_, _>>();
        DesktopLaunchConfig::from_lookup(|name| values.get(name).cloned())
    }

    #[test]
    fn sqlite_environment_selects_persistent_backend_and_document() {
        let config = config(&[
            ("CDITOR_SQLITE_PATH", "/tmp/document.cditor.db"),
            ("CDITOR_DOCUMENT_ID", "42"),
        ])
        .unwrap();
        assert_eq!(
            config.backend,
            DesktopBackendConfig::Sqlite(PathBuf::from("/tmp/document.cditor.db"))
        );

        let cditor = config.into_cditor();
        assert_eq!(cditor.options().document_id, Some(42));
        assert!(matches!(
            &cditor.options().backend,
            CditorBackend::Persistent { provider } if provider.label() == "SQLite"
        ));
    }

    #[test]
    fn postgres_and_sqlite_are_mutually_exclusive() {
        let error = config(&[
            ("CDITOR_SQLITE_PATH", "/tmp/document.cditor.db"),
            ("CDITOR_DATABASE_URL", "postgres://localhost/cditor"),
        ])
        .unwrap_err();
        assert!(error.to_string().contains("cannot both be set"));
    }

    #[test]
    fn persistent_backend_cannot_silently_fall_back_to_demo() {
        let error = config(&[
            ("CDITOR_SQLITE_PATH", "/tmp/document.cditor.db"),
            ("CDITOR_SMALL_DEMO", "true"),
        ])
        .unwrap_err();
        assert!(error.to_string().contains("cannot be combined"));
    }

    #[test]
    fn invalid_document_id_is_rejected() {
        let error = config(&[("CDITOR_DOCUMENT_ID", "not-a-number")]).unwrap_err();
        assert!(error.to_string().contains("CDITOR_DOCUMENT_ID"));
    }

    #[test]
    fn ui_environment_options_are_applied_to_builder() {
        let cditor = config(&[
            ("CDITOR_READONLY", "yes"),
            ("CDITOR_DEBUG_OVERLAY", "1"),
            ("CDITOR_PAYLOAD_WINDOW_SIZE", "64"),
            ("CDITOR_WORKSPACE_ID", "7"),
        ])
        .unwrap()
        .into_cditor();

        assert!(cditor.options().readonly);
        assert!(cditor.options().debug_overlay);
        assert_eq!(cditor.options().payload_window_size, 64);
        assert_eq!(cditor.options().workspace_id, Some(7));
    }

    #[test]
    fn empty_database_variables_do_not_select_persistence() {
        let cditor = config(&[("CDITOR_SQLITE_PATH", ""), ("CDITOR_DATABASE_URL", "")])
            .unwrap()
            .into_cditor();
        assert_eq!(cditor.options().backend, CditorBackend::Demo);
    }
}
