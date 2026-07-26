//! Utility functions for working with skills.

use super::{SkillDescriptor, SkillManager};
use crate::ai::blocklist::view_util::render_provider_icon_button;
use ai::skills::{
    home_skills_path, provider_rank, ParsedSkill, SkillProvider, SKILL_PROVIDER_DEFINITIONS,
};
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use warp_core::ui::appearance::Appearance;
use warp_core::ui::theme::color::internal_colors;
use warp_core::ui::Icon;
use warpui::prelude::MouseStateHandle;
use warpui::EventContext;
use warpui::{AppContext, Element, SingletonEntity};

use crate::warp_managed_paths_watcher::warp_managed_skill_dirs;

/// Deduplicates skills by **name and owning directory**, keeping a single best representative per
/// skill name within each directory.
///
/// Priority rules (when multiple copies share the same skill name):
///
/// 1. **Lower provider rank wins**: following [`SKILL_PROVIDER_DEFINITIONS`] order
///    (index 0 = highest priority), e.g. `Agents > Zap > Claude > …`.
/// 2. **On the same rank, the shorter reference path wins**: used as a stable tiebreak.
///
/// This implementation covers three scenarios:
/// - `npx skills` symlinks a same-name skill into `~/.agents/skills/` /
///   `~/.warp/skills/` / `~/.claude/skills/` (same name, different provider) → keeps
///   the higher-priority provider.
/// - A same-name skill exists in multiple directories at once (e.g. repo root +
///   subdir) → each is kept, letting the caller handle it by path context.
/// - Same name, different content (different provider) → keeps the higher-priority
///   provider.
///
/// Each element of `skill_paths` is a `(dir_path, skill_file_path)` tuple where
/// `dir_path` is the directory that owns the skill and participates in the dedup key.
///
/// **P0-3 prompt cache gap fix**: the returned Vec is sorted lexicographically by
/// `(name, reference)`. Reason: `HashMap::into_values()`'s iteration order is
/// unstable, and this return value goes into the system prompt's skills section —
/// order drift would invalidate the prompt cache across every upstream provider
/// (Anthropic / OpenAI / DeepSeek). Same nature as the P0-3 MCP tools sort.
/// Currently deduplicated by `(name, owning directory)`, so different directories
/// can keep a same-name skill simultaneously. reference still serves as the
/// secondary key for stable sorting, guaranteeing reproducible output order.
#[cfg_attr(not(feature = "local_fs"), allow(dead_code))]
pub(crate) fn unique_skills(
    skill_paths: &[(PathBuf, PathBuf)],
    skills_by_path: &HashMap<PathBuf, ParsedSkill>,
) -> Vec<SkillDescriptor> {
    let mut name_map: HashMap<(String, PathBuf), SkillDescriptor> = HashMap::new();

    for (dir_path, path) in skill_paths {
        let Some(skill) = skills_by_path.get(path) else {
            continue;
        };
        let descriptor = SkillDescriptor::from(skill.clone());
        match name_map.entry((descriptor.name.clone(), dir_path.clone())) {
            Entry::Vacant(e) => {
                e.insert(descriptor);
            }
            Entry::Occupied(mut e) => {
                let new_rank = provider_rank(descriptor.provider);
                let existing_rank = provider_rank(e.get().provider);
                if new_rank < existing_rank
                    || (new_rank == existing_rank
                        && skill_reference_key(&descriptor.reference).len()
                            < skill_reference_key(&e.get().reference).len())
                {
                    e.insert(descriptor);
                }
            }
        }
    }

    let mut out: Vec<SkillDescriptor> = name_map.into_values().collect();
    // P0-3 gap fix: sorted lexicographically by (name, reference literal) to keep the system prompt stable.
    out.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then_with(|| skill_reference_key(&a.reference).cmp(&skill_reference_key(&b.reference)))
    });
    out
}

/// Generates a literal key for `SkillReference`, used for sorting.
/// `Path` uses `to_string_lossy` to avoid cross-platform edge cases; `BundledSkillId`
/// uses the id string directly; the two never collide on the same key (a bundled id
/// contains no path separators).
fn skill_reference_key(reference: &ai::skills::SkillReference) -> String {
    match reference {
        ai::skills::SkillReference::Path(p) => p.to_string_lossy().into_owned(),
        ai::skills::SkillReference::BundledSkillId(id) => id.clone(),
    }
}

/// Lists every skill applicable to the current working directory.
///
/// **Design note**: the old `list_skills_if_changed` sent a diff under the cloud
/// protocol (comparing against `conversation.latest_skills()` sent last round,
/// returning `None` when unchanged) to save on upload tokens — the warp backend
/// maintains session state, so it only needed to be received and kept once on the
/// first round. Now that the project has moved off the cloud, BYOP talks to
/// stateless `/chat/completions` endpoints like OpenAI/Anthropic, and the system
/// prompt is fully re-rendered client-side every round, so the data must be sent
/// every round — otherwise the skills section in the system prompt would vanish
/// starting from the second round. So this is simplified to always returning the
/// full set every round.
pub fn list_skills(working_directory: Option<&Path>, app: &AppContext) -> Vec<SkillDescriptor> {
    SkillManager::as_ref(app).get_skills_for_working_directory(working_directory, app)
}

/// Renders an 'open skill' button for blocklist AI actions and the code diff view.
pub fn render_skill_button<F>(
    button_label: &str,
    button_handle: MouseStateHandle,
    appearance: &Appearance,
    skill_provider: SkillProvider,
    icon_override: Option<Icon>,
    on_click: F,
) -> Box<dyn Element>
where
    F: FnMut(&mut EventContext) + 'static,
{
    let theme = appearance.theme();
    let logo_fill = internal_colors::fg_overlay_6(theme);

    let icon = icon_override.unwrap_or_else(|| skill_provider.icon());

    let color = if icon_override.is_some() {
        logo_fill
    } else {
        skill_provider.icon_fill(logo_fill)
    };

    render_provider_icon_button(
        button_label,
        button_handle,
        appearance,
        icon,
        color,
        on_click,
    )
}

/// Returns a branded icon override for well-known skill names.
pub fn icon_override_for_skill_name(name: &str) -> Option<Icon> {
    match name {
        "stripe-projects-cli" => Some(Icon::StripeLogo),
        _ => None,
    }
}

pub fn skill_path_from_file_path(file_path: &Path) -> Option<PathBuf> {
    for definition in SKILL_PROVIDER_DEFINITIONS.iter() {
        let home_skill_dirs = if definition.provider == SkillProvider::Zap {
            warp_managed_skill_dirs()
        } else {
            home_skills_path(definition.provider).into_iter().collect()
        };
        for home_skills_path in home_skill_dirs {
            if let Ok(relative_path) = file_path.strip_prefix(&home_skills_path) {
                let skill_name = relative_path.components().next()?;
                return Some(home_skills_path.join(skill_name).join("SKILL.md"));
            }
        }
    }
    let path_components: Vec<_> = file_path.components().collect();

    for def in SKILL_PROVIDER_DEFINITIONS.iter() {
        let skill_components: Vec<_> = def.skills_path.components().collect();

        for (idx, window) in path_components.windows(skill_components.len()).enumerate() {
            if window == skill_components.as_slice() {
                let skill_dir = PathBuf::from_iter(
                    file_path
                        .components()
                        .take(idx + skill_components.len() + 1),
                );
                return Some(skill_dir.join("SKILL.md"));
            }
        }
    }
    None
}

#[cfg(test)]
#[path = "skill_utils_tests.rs"]
mod tests;
