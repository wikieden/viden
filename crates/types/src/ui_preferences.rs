#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LocaleId {
    #[serde(rename = "system")]
    System,
    #[serde(rename = "en")]
    En,
    #[serde(rename = "zh-CN")]
    ZhCn,
}

impl LocaleId {
    pub fn from_system_locale(raw: &str) -> Self {
        let normalized = raw.trim().to_ascii_lowercase();
        let language = normalized.split(['_', '-', '.']).next().unwrap_or_default();
        if language == "zh" {
            Self::ZhCn
        } else if language.is_empty() {
            Self::System
        } else {
            Self::En
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiSkin {
    Aurora,
    Ice,
    Mono,
    Amber,
    Phosphor,
}

impl UiSkin {
    pub const ALL: [Self; 5] = [
        Self::Aurora,
        Self::Ice,
        Self::Mono,
        Self::Amber,
        Self::Phosphor,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiColorMode {
    System,
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiDensity {
    Compact,
    Regular,
    Comfy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiMotion {
    System,
    Reduced,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TuiColorDepth {
    Truecolor,
    Ansi256,
    Ansi16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct UiPreferences {
    pub locale: LocaleId,
    pub skin: UiSkin,
    pub mode: UiColorMode,
    pub density: UiDensity,
    pub motion: UiMotion,
}

impl UiPreferences {
    pub fn client_default() -> Self {
        Self {
            locale: LocaleId::System,
            skin: UiSkin::Aurora,
            mode: UiColorMode::System,
            density: UiDensity::Regular,
            motion: UiMotion::System,
        }
    }

    pub fn safe_fallback(locale: LocaleId, motion: UiMotion) -> Self {
        Self {
            locale,
            skin: UiSkin::Aurora,
            mode: UiColorMode::Dark,
            density: UiDensity::Regular,
            motion,
        }
    }

    pub fn is_valid_effective_pair(skin: UiSkin, mode: UiColorMode) -> bool {
        matches!(
            (skin, mode),
            (
                UiSkin::Aurora | UiSkin::Ice | UiSkin::Mono,
                UiColorMode::Dark | UiColorMode::Light
            ) | (UiSkin::Amber | UiSkin::Phosphor, UiColorMode::Dark)
        )
    }
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self::client_default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct UiPreferenceDiagnostic {
    pub code: String,
    pub key: String,
    pub field: Option<String>,
    pub rejected_value: Option<String>,
}

impl UiPreferenceDiagnostic {
    pub fn new(
        code: impl Into<String>,
        key: impl Into<String>,
        field: impl Into<String>,
        rejected_value: Option<String>,
    ) -> Self {
        Self {
            code: code.into(),
            key: key.into(),
            field: Some(field.into()),
            rejected_value,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ResolvedUiPreferences {
    pub locale: LocaleId,
    pub skin: UiSkin,
    pub mode: UiColorMode,
    pub density: UiDensity,
    pub motion: UiMotion,
    pub diagnostics: Vec<UiPreferenceDiagnostic>,
}

impl Default for ResolvedUiPreferences {
    fn default() -> Self {
        // A snapshot must be deterministic even when it predates UI preferences.
        // Resolve the client defaults without host input to produce the built-in
        // English Aurora dark profile while preserving system motion behavior.
        resolve_ui_preferences(None, None, None, UiPreferences::client_default())
    }
}

pub fn resolve_ui_preferences(
    cli: Option<UiPreferences>,
    user: Option<UiPreferences>,
    project: Option<UiPreferences>,
    client: UiPreferences,
) -> ResolvedUiPreferences {
    let requested = cli.or(user).or(project).unwrap_or(client);
    let locale = resolve_locale(requested.locale, client.locale);
    let mode = resolve_mode(requested.mode, client.mode);
    let motion = requested.motion;

    if !UiPreferences::is_valid_effective_pair(requested.skin, mode) {
        let fallback = UiPreferences::safe_fallback(locale, motion);
        return ResolvedUiPreferences {
            locale: fallback.locale,
            skin: fallback.skin,
            mode: fallback.mode,
            density: fallback.density,
            motion: fallback.motion,
            diagnostics: vec![UiPreferenceDiagnostic::new(
                "ui.invalid_skin_mode_pair",
                "skin_mode",
                "ui.mode",
                Some(format!("{:?}/{:?}", requested.skin, mode).to_ascii_lowercase()),
            )],
        };
    }

    ResolvedUiPreferences {
        locale,
        skin: requested.skin,
        mode,
        density: requested.density,
        motion,
        diagnostics: Vec::new(),
    }
}

fn resolve_locale(requested: LocaleId, client: LocaleId) -> LocaleId {
    match requested {
        LocaleId::En | LocaleId::ZhCn => requested,
        LocaleId::System => match client {
            LocaleId::En | LocaleId::ZhCn => client,
            LocaleId::System => LocaleId::En,
        },
    }
}

fn resolve_mode(requested: UiColorMode, client: UiColorMode) -> UiColorMode {
    match requested {
        UiColorMode::Dark | UiColorMode::Light => requested,
        UiColorMode::System => match client {
            UiColorMode::Dark | UiColorMode::Light => client,
            UiColorMode::System => UiColorMode::Dark,
        },
    }
}

/// Largest number of statusbar segments an operator may hide
/// (`ui.layout_preferences`).
///
/// Sixteen: more than the designed statusbar has segments, so a legitimate
/// client can hide every ambient one and still have room, and small enough
/// that the list cannot become an unbounded client-chosen blob riding every
/// snapshot. Over the bound the patch is *refused* rather than clamped — a
/// clamp would silently keep showing a segment the operator asked to hide, and
/// no event would say so.
pub const MAX_HIDDEN_STATUSBAR_SEGMENTS: usize = 16;

/// Largest byte length one hidden-segment name may carry.
///
/// The segment vocabulary belongs to the client, so Core cannot validate the
/// names — only their size. Long enough for any plausible identifier, short
/// enough that the bounded list stays bounded in bytes as well as in rows.
pub const MAX_HIDDEN_STATUSBAR_SEGMENT_BYTES: usize = 64;

/// How the Lane sidebar occupies the cockpit (`D-SIDEBAR`).
///
/// `#[non_exhaustive]`, like every other wire-facing preference enum: a future
/// mode must not break a client's match arms.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum LaneSidebarMode {
    /// The sidebar holds its own column, 176-360px wide.
    Pinned,
    /// The sidebar overlays the centre pane and peeks on hover, leaving the
    /// horizontal space to the transcript.
    ///
    /// The default, per design decision `D-SIDEBAR`: this is what the D1
    /// flagship renders and what an operator who has never opened Settings
    /// sees. It is deliberately *not* the first declared variant, so the
    /// `#[default]` attribute has to state the choice rather than inherit it
    /// from declaration order.
    #[default]
    Floating,
}

/// Cockpit layout preferences Core persists on the operator's behalf.
///
/// This is a *separate* record from [`UiPreferences`] on purpose.
/// [`ResolvedUiPreferences`] is serialized into every `RuntimeSnapshot`, so a
/// new field on it would move the recorded digest of all nine frozen
/// `frontend-contract-v1` base fixtures; a separate record reduces into its
/// own optional view field, absent until Core publishes one, and therefore
/// moves nothing. The trade is deliberate and recorded in
/// `docs/release-0.3.4-contract-design.md`.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct UiLayoutPreferences {
    #[serde(default)]
    pub lane_sidebar_mode: LaneSidebarMode,
    /// Ambient statusbar segments the operator hid.
    ///
    /// Names are kept verbatim, including ones this Core does not recognize:
    /// the statusbar vocabulary belongs to the client, and a Core that stored
    /// only the names it knew would quietly unhide everything a newer client
    /// hid. Identity and actionable segments are never listed here — a client
    /// must not offer to hide the ones that tell an operator who they are or
    /// what needs a decision.
    #[serde(default)]
    pub hidden_statusbar_segments: Vec<String>,
}
