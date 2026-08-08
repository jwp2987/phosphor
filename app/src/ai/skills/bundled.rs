//! Skills bundled with the app: discovery, activation, and variable substitution.
//!
//! Ported from the pinned oracle's `ai/skills/bundled.rs` (`02b53fcd8`), extracted out
//! of `skill_manager.rs` where this logic previously lived inline.
//!
//! Unlike the pin, this fork has no remote-skill daemon: the SSH remote-skill-sync arm
//! (`ai/skills/remote.rs` upstream) is deliberately not built (see `DECLINED.md` / issue
//! #487). The pin's `BundledSkills` wrapper — which multiplexes a local catalog against
//! per-connected-host catalogs keyed by `HostId` — is therefore dropped entirely, along
//! with `SkillPathOrigin`-based dispatch and `LocalOrRemotePath`. `BundledSkill` here is
//! the whole catalog: there is only ever the local host's.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ai::skills::{parse_bundled_skill, ParsedSkill, SkillReference};
use futures::TryStreamExt;
use warp_core::channel::ChannelState;
use warp_core::execution_mode::AppExecutionMode;
use warp_core::features::FeatureFlag;
use warp_core::ui::icons::Icon;
use warp_core::{report_error, safe_warn};
use warpui::{AppContext, SingletonEntity};

use super::SkillDescriptor;
use crate::ai::mcp::{McpIntegration, TemplatableMCPServerManager};
use crate::settings::user_preferences_toml_file_path;

/// Activation condition for a bundled skill.
///
/// `TuiOnly` and `RequiresFeature` ported from the pin's
/// `ai/skills/bundled.rs::BundledSkillActivation` (`02b53fcd8`), issue #370.
#[derive(Debug, Clone)]
pub enum BundledSkillActivation {
    /// Always active.
    Always,
    /// Active only in the TUI frontend.
    TuiOnly,
    /// Active only when a specific Warp feature is enabled.
    RequiresFeature(FeatureFlag),
    /// Active only when a specific MCP server is running.
    RequiresMcp(McpIntegration),
    /// Active only when a specific file exists on disk.
    RequiresFile(PathBuf),
}

impl BundledSkillActivation {
    pub fn is_enabled(&self, ctx: &AppContext) -> bool {
        match self {
            Self::Always => true,
            Self::TuiOnly => AppExecutionMode::as_ref(ctx).is_tui(),
            Self::RequiresFeature(feature) => feature.is_enabled(),
            Self::RequiresMcp(integration) => {
                TemplatableMCPServerManager::as_ref(ctx).is_mcp_server_running(*integration)
            }
            Self::RequiresFile(path) => path.exists(),
        }
    }
}

/// One bundled skill definition with its activation condition and icon.
#[derive(Debug, Clone)]
struct BundledSkillDefinition {
    skill: ParsedSkill,
    activation: BundledSkillActivation,
    icon: Icon,
}

/// Skills bundled into the app.
///
/// Unlike the pin's `BundledSkills`, this *is* the whole catalog — there is no
/// per-remote-host split. See the module doc comment.
#[derive(Debug, Default)]
pub struct BundledSkill {
    definitions: HashMap<String, BundledSkillDefinition>,
}

impl BundledSkill {
    /// Detect all skill definitions bundled with the app for the local host.
    pub async fn detect() -> Self {
        let Some(resources_dir) = warp_core::paths::bundled_resources_dir() else {
            return Self::default();
        };
        let (mut definitions, figma_definitions) = futures::join!(
            load_bundled_skill_definitions(&resources_dir),
            load_figma_skill_definitions(&resources_dir)
        );
        definitions.extend(figma_definitions);
        Self { definitions }
    }

    /// Returns descriptors for bundled skills whose activation conditions are met.
    pub fn active_descriptors(&self, ctx: &AppContext) -> Vec<SkillDescriptor> {
        self.definitions
            .iter()
            .filter(|(_, definition)| definition.activation.is_enabled(ctx))
            .map(|(id, definition)| {
                SkillDescriptor::new_bundled(id.clone(), definition.skill.clone(), definition.icon)
            })
            .collect()
    }

    /// Returns a bundled skill reference when the path belongs to a bundled skill.
    pub fn reference_for_path(&self, path: &Path) -> Option<SkillReference> {
        self.definitions
            .iter()
            .find(|(_, definition)| definition.skill.path.as_path() == path)
            .map(|(id, _)| SkillReference::BundledSkillId(id.clone()))
    }

    /// Returns a bundled skill definition by ID, regardless of activation.
    pub fn skill(&self, id: &str) -> Option<&ParsedSkill> {
        self.definitions.get(id).map(|definition| &definition.skill)
    }

    /// Returns a bundled skill by ID only if its activation condition is met.
    pub fn active_skill(&self, id: &str, ctx: &AppContext) -> Option<&ParsedSkill> {
        let definition = self.definitions.get(id)?;
        definition
            .activation
            .is_enabled(ctx)
            .then_some(&definition.skill)
    }

    /// Iterates every bundled skill definition as `(id, skill)`, regardless of
    /// activation. Used by name-based lookup fallbacks (the BYOP `read_skill` tool
    /// can only see a skill's `<name>`, not its ID).
    pub fn iter(&self) -> impl Iterator<Item = (&str, &ParsedSkill)> {
        self.definitions
            .iter()
            .map(|(id, definition)| (id.as_str(), &definition.skill))
    }

    #[cfg(test)]
    pub fn insert_for_testing(
        &mut self,
        id: impl Into<String>,
        skill: ParsedSkill,
        activation: BundledSkillActivation,
    ) {
        let id = id.into();
        self.definitions.insert(
            id.clone(),
            BundledSkillDefinition {
                skill,
                activation,
                icon: icon_for_bundled_skill(&id),
            },
        );
    }
}

/// Load skill definitions bundled with the app.
async fn load_bundled_skill_definitions(
    resources_dir: &Path,
) -> HashMap<String, BundledSkillDefinition> {
    let skills_dir = resources_dir.join("bundled").join("skills");
    read_bundled_skills(&skills_dir)
        .await
        .into_iter()
        .map(|(id, skill)| {
            let icon = icon_for_bundled_skill(&id);
            let activation = activation_for_bundled_skill(&id, resources_dir);
            (
                id,
                BundledSkillDefinition {
                    skill,
                    activation,
                    icon,
                },
            )
        })
        .collect()
}

/// Load Figma-specific bundled skills from the `figma/` subdirectory.
async fn load_figma_skill_definitions(
    resources_dir: &Path,
) -> HashMap<String, BundledSkillDefinition> {
    let figma_skills_dir = resources_dir
        .join("bundled")
        .join("mcp_skills")
        .join("figma");
    read_bundled_skills(&figma_skills_dir)
        .await
        .into_iter()
        .map(|(id, skill)| {
            (
                id,
                BundledSkillDefinition {
                    skill,
                    activation: BundledSkillActivation::RequiresMcp(McpIntegration::Figma),
                    icon: Icon::Figma,
                },
            )
        })
        .collect()
}

/// Read bundled skill definitions from the specified directory.
pub(crate) async fn read_bundled_skills(skills_dir: &Path) -> HashMap<String, ParsedSkill> {
    let mut skills = HashMap::new();
    let context = build_bundled_skill_context();

    let Ok(mut entries) = async_fs::read_dir(skills_dir).await else {
        return skills;
    };

    while let Ok(Some(entry)) = entries.try_next().await {
        let entry_path = entry.path();
        if !entry_path.is_dir() {
            continue;
        }

        let skill_file_path = entry_path.join("SKILL.md");
        let mut skill = match parse_bundled_skill(&skill_file_path) {
            Ok(skill) => skill,
            Err(err) => {
                report_error!(err.context(format!(
                    "Failed to parse bundled skill at {}",
                    skill_file_path.display()
                )));
                continue;
            }
        };

        // We use the directory name as the skill ID (guaranteed unique within bundled skills).
        let Some(skill_id) = entry_path.file_name().and_then(|s| s.to_str()) else {
            safe_warn!(
                safe: ("Could not resolve bundled skill ID, skipping skill"),
                full: ("Could not resolve bundled skill ID from {}, skipping skill", skill.path.display())
            );
            continue;
        };

        // Apply variable substitution to the skill content.
        skill.content = handlebars::render_template(&skill.content, &context);
        skills.insert(skill_id.to_owned(), skill);
    }

    log::info!("Read {} bundled skills", skills.len());

    skills
}

/// Builds the context map for bundled skill variable substitution.
///
/// Supported variables:
/// - `{{warp_server_url}}` - Empty in this fork; retained for bundled skill compatibility.
/// - `{{warp_cli_binary_name}}` - The CLI binary name (e.g., `warp` or `warp-cli`)
/// - `{{warp_url_scheme}}` - The URL scheme (e.g., `warp`, `warpdev`, `warppreview`)
/// - `{{settings_schema_path}}` - Path to the bundled JSON settings schema
/// - `{{settings_file_path}}` - Path to the user's settings TOML file
pub(crate) fn build_bundled_skill_context() -> HashMap<String, String> {
    let mut context: HashMap<String, String> = [
        ("warp_server_url".to_owned(), String::new()),
        (
            "warp_cli_binary_name".to_owned(),
            ChannelState::channel().cli_command_name().to_owned(),
        ),
        (
            "warp_url_scheme".to_owned(),
            ChannelState::url_scheme().to_owned(),
        ),
        (
            "settings_file_path".to_owned(),
            user_preferences_toml_file_path().display().to_string(),
        ),
    ]
    .into_iter()
    .collect();

    if let Some(schema_path) =
        warp_core::paths::bundled_resources_dir().map(|dir| dir.join("settings_schema.json"))
    {
        context.insert(
            "settings_schema_path".to_owned(),
            schema_path.display().to_string(),
        );
    }

    context
}

/// Returns the icon for a bundled skill, given its directory-based ID.
/// Skills with a known brand (e.g. `pr-comments` → GitHub) get a
/// branded icon; everything else falls back to the app logo.
pub(crate) fn icon_for_bundled_skill(skill_id: &str) -> Icon {
    match skill_id {
        "pr-comments" => Icon::Github,
        _ => Icon::WarpLogoLight,
    }
}

/// Returns the activation condition for a bundled skill.
///
/// Most skills are always active. Skills that depend on a bundled resource
/// file use `RequiresFile` so they only appear when the resource is present.
///
/// The pin also gates two bundled skills here that this fork has no directory
/// for: `tui-migrate-setup` (on `AppExecutionMode::is_tui`) and `warpctrl` (on
/// `FeatureFlag::WarpControlCli`). Both arms are omitted because
/// `resources/bundled/skills/` ships neither skill — an activation arm with no
/// skill to drive it is unreachable, untested code. The flag and the underlying
/// `RequiresFeature` mechanism both exist and are exercised by
/// `read_skill_tests.rs`; only the skill content is missing. Porting the skill
/// directories is #370.
pub(crate) fn activation_for_bundled_skill(
    skill_id: &str,
    resources_dir: &Path,
) -> BundledSkillActivation {
    match skill_id {
        "modify-settings" => {
            BundledSkillActivation::RequiresFile(resources_dir.join("settings_schema.json"))
        }
        _ => BundledSkillActivation::Always,
    }
}

#[cfg(test)]
#[path = "bundled_tests.rs"]
mod tests;
