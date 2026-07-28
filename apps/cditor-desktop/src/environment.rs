use std::{env, ffi::OsString, fmt};

use cditor_sdk::Cditor;

const DEFAULT_PAYLOAD_WINDOW_SIZE: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
struct DesktopLaunchConfig {
    large_demo: bool,
    readonly: bool,
    debug_overlay: bool,
    payload_window_size: usize,
}

impl DesktopLaunchConfig {
    fn from_lookup(
        mut lookup: impl FnMut(&str) -> Option<OsString>,
    ) -> Result<Self, DesktopEnvironmentError> {
        let small_demo = parse_bool("CDITOR_SMALL_DEMO", lookup("CDITOR_SMALL_DEMO"))?;
        let large_demo = parse_bool("CDITOR_LARGE_DEMO", lookup("CDITOR_LARGE_DEMO"))?;
        if small_demo && large_demo {
            return Err(DesktopEnvironmentError::new(
                "CDITOR_SMALL_DEMO and CDITOR_LARGE_DEMO cannot both be enabled",
            ));
        }

        Ok(Self {
            large_demo,
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
        let cditor = if self.large_demo {
            Cditor::new().large_demo()
        } else {
            Cditor::new().demo()
        };
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

    use cditor_sdk::CditorDocumentSource;

    use super::*;

    fn config(entries: &[(&str, &str)]) -> Result<DesktopLaunchConfig, DesktopEnvironmentError> {
        let values = entries
            .iter()
            .map(|(name, value)| ((*name).to_owned(), OsString::from(value)))
            .collect::<HashMap<_, _>>();
        DesktopLaunchConfig::from_lookup(|name| values.get(name).cloned())
    }

    #[test]
    fn ui_environment_options_are_applied() {
        let cditor = config(&[
            ("CDITOR_READONLY", "yes"),
            ("CDITOR_DEBUG_OVERLAY", "1"),
            ("CDITOR_PAYLOAD_WINDOW_SIZE", "64"),
            ("CDITOR_LARGE_DEMO", "true"),
        ])
        .unwrap()
        .into_cditor();

        assert!(cditor.options().readonly);
        assert!(cditor.options().debug_overlay);
        assert_eq!(cditor.options().payload_window_size, 64);
        assert_eq!(cditor.options().source, CditorDocumentSource::LargeDemo);
    }

    #[test]
    fn desktop_defaults_to_small_demo() {
        let cditor = config(&[]).unwrap().into_cditor();
        assert_eq!(cditor.options().source, CditorDocumentSource::Demo);
    }
}
