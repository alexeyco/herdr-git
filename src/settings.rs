//! Pane layout settings read from the gitui plugin's `config.toml`.
//!
//! Action mode calls `Settings::load()` once per launch, before the pane
//! open, in `run_action()`. The settings layer never fails the action: every
//! degradation path (no config directory, missing or unreadable file, corrupt
//! content, bad values) returns documented defaults with warnings on stderr.

use std::path::{Path, PathBuf};

use crate::adapters::herdr;

/// Where herdr places the gitui pane. Action mode maps the variants to the
/// `--placement` argv values `tab`, `split`, `overlay`, `zoomed`, `popup`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Placement {
    /// Open in a new tab (the default).
    #[default]
    Tab,
    /// Open as a split pane next to the caller.
    Split,
    /// Open as an overlay pane.
    Overlay,
    /// Open zoomed over the caller's tab.
    Zoomed,
    /// Open as a centered session-modal popup; the tiled layout is unchanged
    /// and the popup closes when gitui quits.
    Popup,
}

/// Which way a split pane opens. Action mode maps `Right`/`Down` to the
/// `--direction` argv value and `Auto` to "do not pass the flag at all".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Direction {
    /// Open to the right of the caller (the default).
    #[default]
    Right,
    /// Open below the caller.
    Down,
    /// Let herdr choose; no `--direction` is passed.
    Auto,
}

/// Flat-key layout settings. The defaults mirror the documented value sets:
/// `placement = "tab"`, `direction = "right"`, and both popup size keys
/// unset (herdr renders its default half-size popup).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Settings {
    /// The `placement` key.
    pub(crate) placement: Placement,
    /// The `direction` key; only meaningful for `split` placement.
    pub(crate) direction: Direction,
    /// The `width` key: resolved argv token for the popup `--width` flag
    /// (positive integer cells or an `N%` string, kept verbatim). Only
    /// meaningful for `popup`; unset when omitted or invalid.
    pub(crate) width: Option<String>,
    /// The `height` key, same value set as `width`. Only meaningful for
    /// `popup`.
    pub(crate) height: Option<String>,
}

impl Settings {
    /// Single entry point: resolve the config directory, read
    /// `<dir>/config.toml` exactly once, and parse it. Never fails — all
    /// failures degrade to defaults with warnings printed on stderr.
    ///
    /// Called exactly once per action launch by `run_action()`, before the
    /// pane open. Pane mode never calls it.
    pub(crate) fn load() -> Settings {
        let (settings, warnings) = match resolve_config_dir() {
            Ok(dir) => load_from_dir(&dir),
            Err(reason) => (
                Settings::default(),
                vec![format!(
                    "config directory unavailable: {reason}; using default settings"
                )],
            ),
        };
        for warning in warnings {
            eprintln!("herdr-gitui: warning: {warning}");
        }
        settings
    }
}

/// Resolves the settings directory: a non-blank `HERDR_PLUGIN_CONFIG_DIR`
/// wins outright and no callback is made; otherwise the HERDR_BIN_PATH-
/// guarded `plugin config-dir alexeyco.git` query answers. The `Err` reason maps to
/// the single "config directory unavailable" warning plus defaults.
fn resolve_config_dir() -> Result<PathBuf, String> {
    if let Some(dir) = env_non_blank("HERDR_PLUGIN_CONFIG_DIR") {
        return Ok(PathBuf::from(dir));
    }
    herdr::plugin_config_dir().map(PathBuf::from)
}

fn env_non_blank(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Reads `<dir>/config.toml` once. A file that does not exist is silent
/// defaults; an existing-but-unreadable file degrades like corrupt content
/// with one warning. Returns the settings plus the warnings for the caller
/// to print.
fn load_from_dir(dir: &Path) -> (Settings, Vec<String>) {
    let path = dir.join("config.toml");
    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return (Settings::default(), Vec::new());
        }
        Err(e) => {
            return (
                Settings::default(),
                vec![format!(
                    "cannot read config.toml at {}: {e}; using default settings",
                    path.display()
                )],
            );
        }
    };
    parse_settings(&content)
}

/// Two-phase parsing: first the whole file as a flat TOML table — unparseable
/// content is all defaults with one warning — then per-key validation so an
/// unknown key or a bad value only ever falls back to that key's own default
/// while the other keys keep their effect.
fn parse_settings(content: &str) -> (Settings, Vec<String>) {
    let table: toml::Table = match toml::from_str(content) {
        Ok(table) => table,
        Err(e) => {
            return (
                Settings::default(),
                vec![format!("invalid config.toml: {e}; using default settings")],
            );
        }
    };
    let mut settings = Settings::default();
    let mut warnings = Vec::new();
    for (key, value) in &table {
        match key.as_str() {
            "placement" => match parse_placement(value) {
                Some(placement) => settings.placement = placement,
                None => {
                    warnings.push(invalid_value_warning(
                        "placement",
                        value,
                        "tab|split|overlay|zoomed|popup",
                        "tab",
                    ));
                }
            },
            "direction" => match parse_direction(value) {
                Some(direction) => settings.direction = direction,
                None => {
                    warnings.push(invalid_value_warning(
                        "direction",
                        value,
                        "right|down|auto",
                        "right",
                    ));
                }
            },
            "width" => match parse_size(value) {
                Some(width) => settings.width = Some(width),
                None => warnings.push(invalid_size_warning("width", value)),
            },
            "height" => match parse_size(value) {
                Some(height) => settings.height = Some(height),
                None => warnings.push(invalid_size_warning("height", value)),
            },
            other => warnings.push(format!("unknown key in config.toml: {other}")),
        }
    }
    (settings, warnings)
}

fn parse_placement(value: &toml::Value) -> Option<Placement> {
    match value.as_str() {
        Some("tab") => Some(Placement::Tab),
        Some("split") => Some(Placement::Split),
        Some("overlay") => Some(Placement::Overlay),
        Some("zoomed") => Some(Placement::Zoomed),
        Some("popup") => Some(Placement::Popup),
        _ => None,
    }
}

fn parse_direction(value: &toml::Value) -> Option<Direction> {
    match value.as_str() {
        Some("right") => Some(Direction::Right),
        Some("down") => Some(Direction::Down),
        Some("auto") => Some(Direction::Auto),
        _ => None,
    }
}

/// One popup size key: valid either as a positive integer TOML value (outer
/// terminal cells) or as an `N%` string (one or more ASCII digits and a `%`,
/// value greater than zero). The resolved value is the argv token passed
/// verbatim to herdr (`80` or `80%`); values below the popup minimum are
/// clamped by the daemon, not validated here. Everything else — zero,
/// negative, float, bool, or a string that is not the `N%` form (a bare
/// `"80"` included, per proposal ASSUMPTION) — is invalid, so the key stays
/// unset.
fn parse_size(value: &toml::Value) -> Option<String> {
    if let Some(cells) = value.as_integer() {
        return if cells > 0 {
            Some(cells.to_string())
        } else {
            None
        };
    }
    let text = value.as_str()?;
    let digits = text.strip_suffix('%')?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let percent: u128 = digits.parse().ok()?;
    if percent > 0 {
        Some(text.to_string())
    } else {
        None
    }
}

/// Warning for an invalid `width`/`height` value: the key is left unset (its
/// "default" is no flag), so the wording names the accepted value set instead
/// of a fallback value.
fn invalid_size_warning(key: &str, value: &toml::Value) -> String {
    let shown = match (value.as_str(), value.as_integer()) {
        (Some(s), _) => format!("\"{s}\""),
        (None, Some(n)) => n.to_string(),
        (None, None) => "non-integer value".to_string(),
    };
    format!(
        "invalid {key} in config.toml: {shown}; expected a positive integer or an \"N%\" string, leaving {key} unset"
    )
}

/// Warning for an invalid or non-string value of a known key: the value
/// shown is quoted for strings and generic for other TOML types, and names
/// the per-key default that takes over.
fn invalid_value_warning(key: &str, value: &toml::Value, allowed: &str, fallback: &str) -> String {
    let shown = match value.as_str() {
        Some(s) => format!("\"{s}\""),
        None => "non-string value".to_string(),
    };
    format!(
        "invalid {key} in config.toml: {shown}; expected one of {allowed}, using default {key} {fallback}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Serializes tests that mutate the settings environment.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    const ENV_KEYS: [&str; 2] = ["HERDR_PLUGIN_CONFIG_DIR", "HERDR_BIN_PATH"];

    /// A scratch directory removed when the test ends.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let dir = std::env::temp_dir().join(format!(
                "herdr-gitui-settings-test-{tag}-{}-{nanos}",
                std::process::id()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn write_config(&self, content: &str) {
            std::fs::write(self.0.join("config.toml"), content).unwrap();
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Sets both settings env keys for the duration of a test, restoring the
    /// previous values on drop.
    struct ConfigEnv {
        _lock: std::sync::MutexGuard<'static, ()>,
        prev: Vec<(String, Option<OsString>)>,
    }

    impl ConfigEnv {
        fn set(config_dir: Option<&str>, bin_path: Option<&str>) -> Self {
            let lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let prev = ENV_KEYS
                .iter()
                .map(|k| (k.to_string(), std::env::var_os(k)))
                .collect();
            set_var("HERDR_PLUGIN_CONFIG_DIR", config_dir);
            set_var("HERDR_BIN_PATH", bin_path);
            Self { _lock: lock, prev }
        }
    }

    impl Drop for ConfigEnv {
        fn drop(&mut self) {
            for (key, value) in &self.prev {
                match value {
                    Some(v) => unsafe { std::env::set_var(key, v) },
                    None => unsafe { std::env::remove_var(key) },
                }
            }
        }
    }

    fn set_var(key: &str, value: Option<&str>) {
        match value {
            Some(v) => unsafe { std::env::set_var(key, v) },
            None => unsafe { std::env::remove_var(key) },
        }
    }

    #[test]
    fn defaults_with_no_file_are_silent() {
        let dir = TempDir::new("no-file");
        let (settings, warnings) = load_from_dir(dir.path());
        assert_eq!(settings, Settings::default());
        assert!(warnings.is_empty(), "{warnings:?}");
        // A directory that does not exist contains no config.toml either.
        let (settings, warnings) = load_from_dir(&dir.path().join("gone"));
        assert_eq!(settings, Settings::default());
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    #[test]
    fn empty_file_is_silent_defaults() {
        let (settings, warnings) = parse_settings("");
        assert_eq!(settings, Settings::default());
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    #[test]
    fn explicit_valid_values_are_applied() {
        let dir = TempDir::new("valid");
        dir.write_config("placement = \"zoomed\"\ndirection = \"auto\"\n");
        let (settings, warnings) = load_from_dir(dir.path());
        assert_eq!(settings.placement, Placement::Zoomed);
        assert_eq!(settings.direction, Direction::Auto);
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    #[test]
    fn corrupt_toml_falls_back_to_defaults_with_one_warning() {
        let dir = TempDir::new("corrupt");
        dir.write_config("placement = ");
        let (settings, warnings) = load_from_dir(dir.path());
        assert_eq!(settings, Settings::default());
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("invalid config.toml"), "{warnings:?}");
    }

    /// Spec scenario "Existing but unreadable file": a `config.toml` that is
    /// a directory reproduces the non-`NotFound` read error deterministically
    /// (open/read fails with EISDIR on every platform), with no chmod or root
    /// fragility. One warning plus defaults, like corrupt content.
    #[test]
    fn existing_but_unreadable_config_degrades_with_one_warning() {
        let dir = TempDir::new("unreadable");
        std::fs::create_dir(dir.path().join("config.toml")).unwrap();
        let (settings, warnings) = load_from_dir(dir.path());
        assert_eq!(settings, Settings::default());
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(
            warnings[0].contains("cannot read config.toml"),
            "{warnings:?}"
        );
    }

    #[test]
    fn unknown_key_warns_but_known_keys_keep_effect() {
        let dir = TempDir::new("unknown-key");
        dir.write_config("focus = false\nplacement = \"split\"\n");
        let (settings, warnings) = load_from_dir(dir.path());
        assert_eq!(settings.placement, Placement::Split);
        assert_eq!(settings.direction, Direction::Right);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(
            warnings[0].contains("unknown key") && warnings[0].contains("focus"),
            "{warnings:?}"
        );
    }

    #[test]
    fn invalid_value_falls_back_per_key() {
        // Spec scenario: `placement = "zigzag"` degrades to the default
        // placement while `direction = "down"` stays valid and applied.
        let dir = TempDir::new("invalid-value");
        dir.write_config("placement = \"zigzag\"\ndirection = \"down\"\n");
        let (settings, warnings) = load_from_dir(dir.path());
        assert_eq!(settings.placement, Placement::Tab);
        assert_eq!(settings.direction, Direction::Down);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(
            warnings[0].contains("invalid placement") && warnings[0].contains("zigzag"),
            "{warnings:?}"
        );
    }

    #[test]
    fn non_string_value_falls_back_to_key_default() {
        let (settings, warnings) = parse_settings("placement = 42\ndirection = \"auto\"\n");
        assert_eq!(settings.placement, Placement::Tab);
        assert_eq!(settings.direction, Direction::Auto);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(
            warnings[0].contains("invalid placement") && warnings[0].contains("non-string value"),
            "{warnings:?}"
        );
    }

    #[test]
    fn popup_placement_is_accepted() {
        let dir = TempDir::new("popup-placement");
        dir.write_config("placement = \"popup\"\n");
        let (settings, warnings) = load_from_dir(dir.path());
        assert_eq!(settings.placement, Placement::Popup);
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    #[test]
    fn size_keys_default_unset() {
        // Both size keys omitted → unset, and no file at all is the same.
        let (settings, warnings) = parse_settings("placement = \"popup\"\n");
        assert_eq!(settings.width, None);
        assert_eq!(settings.height, None);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(Settings::default().width, None);
        assert_eq!(Settings::default().height, None);
    }

    #[test]
    fn integer_cells_and_percentage_string_are_valid() {
        // Spec scenario "Integer cells and percentage string are valid":
        // `width = 80` and `height = "80%"` resolve with no warning, kept
        // verbatim as the argv tokens `80` and `80%`.
        let dir = TempDir::new("size-valid");
        dir.write_config("placement = \"popup\"\nwidth = 80\nheight = \"80%\"\n");
        let (settings, warnings) = load_from_dir(dir.path());
        assert_eq!(settings.placement, Placement::Popup);
        assert_eq!(settings.width.as_deref(), Some("80"));
        assert_eq!(settings.height.as_deref(), Some("80%"));
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    #[test]
    fn invalid_size_value_omits_that_key_with_warning() {
        // Spec scenario "Invalid size value omits that key with a warning":
        // `width = 0` warns and stays unset while `height = "80%"` keeps
        // effect, and the size keys never trip the unknown-key warning.
        // The placement value resolves too, so the warning below is the only
        // one and can only come from `width = 0`.
        let dir = TempDir::new("size-invalid");
        dir.write_config("placement = \"popup\"\nwidth = 0\nheight = \"80%\"\n");
        let (settings, warnings) = load_from_dir(dir.path());
        assert_eq!(settings.placement, Placement::Popup);
        assert_eq!(settings.width, None);
        assert_eq!(settings.height.as_deref(), Some("80%"));
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("invalid width"), "{warnings:?}");
    }

    #[test]
    fn size_keys_accepted_for_non_popup_placements() {
        // The size keys are known keys: with `placement = "split"` they parse
        // cleanly with no warning (ignoring them in the argv is the herdr
        // layer's job, pinned by the pane-launch scenarios).
        let (settings, warnings) =
            parse_settings("placement = \"split\"\nwidth = 80\nheight = \"80%\"\n");
        assert_eq!(settings.placement, Placement::Split);
        assert_eq!(settings.width.as_deref(), Some("80"));
        assert_eq!(settings.height.as_deref(), Some("80%"));
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    /// Resolves one TOML literal as the `width` key.
    fn parsed_size(literal: &str) -> Option<String> {
        let table: toml::Table =
            toml::from_str(&format!("width = {literal}")).expect("valid TOML for one literal");
        parse_size(table.get("width").expect("the width key"))
    }

    #[test]
    fn parse_size_accepts_positive_integers_and_percent_strings() {
        assert_eq!(parsed_size("80").as_deref(), Some("80"));
        assert_eq!(parsed_size("1").as_deref(), Some("1"));
        assert_eq!(parsed_size("\"80%\"").as_deref(), Some("80%"));
        assert_eq!(parsed_size("\"1%\"").as_deref(), Some("1%"));
        // Values below herdr's popup minimum are clamped by the daemon.
        assert_eq!(parsed_size("\"1000%\"").as_deref(), Some("1000%"));
    }

    #[test]
    fn parse_size_rejects_everything_else() {
        // Zero, negative, float, bool, non-string, and any string that is
        // not the `N%` shape — a bare numeric string included (proposal
        // ASSUMPTION).
        assert_eq!(parsed_size("0"), None);
        assert_eq!(parsed_size("-3"), None);
        assert_eq!(parsed_size("1.5"), None);
        assert_eq!(parsed_size("true"), None);
        assert_eq!(parsed_size("\"80\"").as_deref(), None);
        assert_eq!(parsed_size("\"tall\"").as_deref(), None);
        assert_eq!(parsed_size("\"0%\"").as_deref(), None);
        assert_eq!(parsed_size("\"%\"").as_deref(), None);
        assert_eq!(parsed_size("\"80 %\"").as_deref(), None);
        assert_eq!(parsed_size("\"-80%\"").as_deref(), None);
    }

    #[test]
    fn environment_variable_preferred_over_callback() {
        // Spec scenario "Environment variable preferred": with the env var
        // set, resolution must succeed from the env value alone. Pointing
        // HERDR_BIN_PATH at a binary that cannot answer the callback proves
        // the callback is not made.
        let dir = TempDir::new("env-preferred");
        dir.write_config("placement = \"split\"\n");
        let _env = ConfigEnv::set(
            Some(dir.path().to_str().unwrap()),
            Some("/nonexistent/herdr-binary"),
        );
        let resolved = resolve_config_dir().expect("env-provided directory resolves");
        assert_eq!(resolved, dir.path());
        let (settings, warnings) = load_from_dir(&resolved);
        assert_eq!(settings.placement, Placement::Split);
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    #[test]
    fn environment_variable_value_is_trimmed() {
        let dir = TempDir::new("env-trim");
        let padded = format!("  {}  ", dir.path().display());
        let _env = ConfigEnv::set(Some(&padded), Some("/nonexistent/herdr-binary"));
        let resolved = resolve_config_dir().expect("trimmed directory resolves");
        assert_eq!(resolved, dir.path());
    }

    #[test]
    fn blank_env_and_missing_bin_path_yield_no_directory() {
        // Spec scenario "Neither source yields a directory": blank env var
        // plus no HERDR_BIN_PATH fails resolution with the guard's reason;
        // `Settings::load` maps that to defaults plus one warning.
        let _env = ConfigEnv::set(Some("   "), None);
        let err = resolve_config_dir().expect_err("no usable directory");
        assert!(err.contains("HERDR_BIN_PATH"), "{err}");
    }
}
