use crate::appearance::Appearance;
use crate::drive::cloud_object_styling::warp_drive_icon_color;
use crate::drive::DriveObjectType;
use crate::features::FeatureFlag;
use crate::search::command_palette::mixer::CommandPaletteItemAction;
use crate::search::command_palette::render_util::{
    colors, render_search_item_icon, render_search_item_icon_placeholder,
};
use crate::search::item::SearchItem;
use crate::search::result_renderer::ItemHighlightState;
use crate::ui_components::icons::Icon;
use crate::util::bindings::{BindingGroup, CommandBinding};
use fuzzy_match::FuzzyMatchResult;
use ordered_float::OrderedFloat;
use pathfinder_color::ColorU;
use std::sync::Arc;
use warpui::elements::{
    Align, ConstrainedBox, Container, Flex, Highlight, ParentElement, Shrinkable, Text,
};
use warpui::fonts::{Properties, Weight};
use warpui::keymap::{DescriptionContext, Keystroke};
use warpui::ui_components::components::UiComponent;
use warpui::{AppContext, Element, SingletonEntity};

/// A matched binding from a search query.
#[derive(Debug)]
pub struct MatchedBinding {
    fuzzy_match_result: FuzzyMatchResult,
    binding: Arc<CommandBinding>,
    /// The query is a case-insensitive prefix of this action's visible name.
    /// Set by the data source, which is where the query is in scope.
    name_is_prefix_match: bool,
}

impl MatchedBinding {
    pub fn new(fuzzy_match_result: FuzzyMatchResult, binding: Arc<CommandBinding>) -> Self {
        Self {
            fuzzy_match_result,
            binding,
            name_is_prefix_match: false,
        }
    }

    /// As [`Self::new`], recording whether `query` is a case-insensitive prefix
    /// of the action's visible name. See [`SearchItem::priority_tier`] below.
    pub fn new_with_query(
        fuzzy_match_result: FuzzyMatchResult,
        binding: Arc<CommandBinding>,
        query: &str,
    ) -> Self {
        let query = query.trim().to_lowercase();
        let name_is_prefix_match = !query.is_empty()
            && binding
                .description
                .in_context(DescriptionContext::Default)
                .to_lowercase()
                .starts_with(&query);
        Self {
            fuzzy_match_result,
            binding,
            name_is_prefix_match,
        }
    }

    /// Creates a new placeholder [`MatchedBinding`] using `name` as the [`CommandBinding`] name.
    pub fn placeholder(name: String) -> Self {
        Self::new(
            FuzzyMatchResult::no_match(),
            Arc::new(CommandBinding::placeholder(name)),
        )
    }

    pub fn render(
        &self,
        highlight_state: ItemHighlightState,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let label = self.render_label(highlight_state, appearance);
        let mut binding = Flex::row();

        binding.add_child(Shrinkable::new(1., Align::new(label).left().finish()).finish());

        if let Some(trigger) = self.binding.trigger.clone() {
            let shortcut = appearance.ui_builder().keyboard_shortcut(&trigger).build();
            binding.add_child(
                Container::new(shortcut.finish())
                    .with_margin_right(styles::KEYBINDING_MARGIN_RIGHT)
                    .finish(),
            );
        }
        ConstrainedBox::new(binding.finish())
            .with_height(styles::SEARCH_ITEM_HEIGHT)
            .finish()
    }

    fn render_label(
        &self,
        item_highlight_state: ItemHighlightState,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        Text::new_inline(
            self.binding
                .description
                .in_context(DescriptionContext::Default)
                .to_owned(),
            appearance.ui_font_family(),
            appearance.monospace_font_size(),
        )
        .with_color(item_highlight_state.sub_text_fill(appearance).into_solid())
        .with_style(Properties::default().weight(Weight::Bold))
        .with_single_highlight(
            Highlight::new()
                .with_properties(Properties::default().weight(Weight::Bold))
                .with_foreground_color(
                    item_highlight_state.main_text_fill(appearance).into_solid(),
                ),
            self.fuzzy_match_result.matched_indices.clone(),
        )
        .finish()
    }
}

impl SearchItem for MatchedBinding {
    type Action = CommandPaletteItemAction;

    fn render_icon(
        &self,
        highlight_state: ItemHighlightState,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        match self.binding.group {
            None => render_search_item_icon_placeholder(appearance),
            Some(group) => render_search_item_icon(
                appearance,
                group.icon(),
                group.icon_color(appearance),
                highlight_state,
            ),
        }
    }

    fn render_item(
        &self,
        highlight_state: ItemHighlightState,
        app: &AppContext,
    ) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        self.render(highlight_state, appearance)
    }

    fn render_details(&self, _: &AppContext) -> Option<Box<dyn Element>> {
        // Bindings do not support details panels.
        None
    }

    /// Rank an action whose visible name the query is a prefix of above
    /// everything else in the palette.
    ///
    /// The palette searches sessions alongside actions, and a session's title is
    /// its working directory. A long path can accumulate enough fuzzy hits to
    /// outscore an exact title match: querying "Activate Previous Pane" in a
    /// checkout containing `.../tmp/test_pane_group_state_multi_pane` selected
    /// the *session*, and enter switched sessions instead of running the action
    /// (#607). Both sat in tier 0, so raw fuzzy score alone decided it.
    ///
    /// Tiers are compared before scores and, after `SearchBar`'s `.rev()` for
    /// `SearchResultOrdering::TopDown`, a higher tier sorts first -- the same
    /// mechanism `DiffSetSearchItem` uses to "prioritize diffsets above other
    /// items". Only a prefix match qualifies, so ordinary fuzzy queries rank
    /// exactly as before.
    fn priority_tier(&self) -> u8 {
        if self.name_is_prefix_match {
            1
        } else {
            0
        }
    }

    fn score(&self) -> OrderedFloat<f64> {
        OrderedFloat(self.fuzzy_match_result.score as f64)
    }

    fn accept_result(&self) -> Self::Action {
        CommandPaletteItemAction::AcceptBinding {
            binding: self.binding.clone(),
        }
    }

    fn execute_result(&self) -> Self::Action {
        self.accept_result()
    }

    fn accessibility_label(&self) -> String {
        let trigger = self.binding.trigger.as_ref();

        format!(
            "Selected {}, {}.",
            &self
                .binding
                .description
                .in_context(DescriptionContext::Default),
            trigger.map(Keystroke::normalized).unwrap_or_default()
        )
    }

    fn accessibility_help_message(&self) -> Option<String> {
        self.binding
            .trigger
            .as_ref()
            .map_or("Press enter to confirm.".into(), |trigger| {
                format!(
                    "Press enter to confirm. Use {} binding to run this action in the future.",
                    trigger.normalized()
                )
            })
            .into()
    }
}

/// Trait to compute an icon for a search item.
trait SearchItemIcon {
    fn icon(&self) -> Icon;

    fn icon_color(&self, appearance: &Appearance) -> ColorU;
}

impl SearchItemIcon for BindingGroup {
    fn icon(&self) -> Icon {
        match self {
            Self::Settings => Icon::Gear,
            Self::WarpAi => {
                if !FeatureFlag::AgentMode.is_enabled() {
                    Icon::AiAssistant
                } else {
                    Icon::Oz
                }
            }
            Self::Close => Icon::X,
            Self::Navigation => Icon::Navigation,
            Self::Workflow => Icon::Workflow,
            Self::Notebooks => Icon::Notebook,
            Self::Folders => Icon::Folder,
            Self::KeyboardShortcuts => Icon::Keyboard,
            Self::AutoUpdate => Icon::AutoUpdate,
            Self::Notifications => Icon::Bell,
            Self::EnvVarCollection => Icon::EnvVarCollection,
            Self::Terminal => Icon::Terminal,
        }
    }

    fn icon_color(&self, appearance: &Appearance) -> ColorU {
        match self {
            Self::Settings
            | Self::Navigation
            | Self::Close
            | Self::KeyboardShortcuts
            | Self::AutoUpdate
            | Self::Folders
            | Self::Terminal
            | Self::Notifications => appearance.theme().foreground().into_solid(),
            Self::WarpAi if !FeatureFlag::AgentMode.is_enabled() => {
                ColorU::from_u32(colors::WARP_AI)
            }
            Self::WarpAi => appearance.theme().foreground().into_solid(),
            Self::Workflow => warp_drive_icon_color(appearance, DriveObjectType::Workflow),
            Self::Notebooks => warp_drive_icon_color(
                appearance,
                DriveObjectType::Notebook {
                    is_ai_document: false,
                },
            ),
            Self::EnvVarCollection => {
                warp_drive_icon_color(appearance, DriveObjectType::EnvVarCollection)
            }
        }
    }
}

pub(crate) mod styles {
    /// Total height of the search item.
    pub const SEARCH_ITEM_HEIGHT: f32 = 40.;

    /// Margin between the right-side of the element and the end of the keybinding.
    pub const KEYBINDING_MARGIN_RIGHT: f32 = 14.;
}

#[cfg(test)]
mod priority_tier_tests {
    use super::MatchedBinding;
    use crate::search::item::SearchItem;
    use crate::util::bindings::CommandBinding;
    use fuzzy_match::FuzzyMatchResult;
    use std::sync::Arc;

    fn binding(description: &str) -> Arc<CommandBinding> {
        Arc::new(CommandBinding::new(
            "pane_group:navigate_prev".to_string(),
            description.to_string(),
            None,
        ))
    }

    /// Querying an action's visible name must rank that action above everything
    /// else, including a session whose path happens to fuzzy-match better (#607).
    #[test]
    fn query_matching_an_action_name_prefix_gets_the_higher_tier() {
        let matched = MatchedBinding::new_with_query(
            FuzzyMatchResult::no_match(),
            binding("Activate Previous Pane"),
            "Activate Previous Pane",
        );
        assert_eq!(matched.priority_tier(), 1);

        let partial = MatchedBinding::new_with_query(
            FuzzyMatchResult::no_match(),
            binding("Activate Previous Pane"),
            "activate prev",
        );
        assert_eq!(
            partial.priority_tier(),
            1,
            "a prefix of the name, case-insensitively, still qualifies"
        );
    }

    /// A fuzzy hit that is NOT a prefix of the name keeps the default tier, so
    /// ordinary queries rank exactly as they did before.
    #[test]
    fn fuzzy_but_non_prefix_match_keeps_the_default_tier() {
        let matched = MatchedBinding::new_with_query(
            FuzzyMatchResult::no_match(),
            binding("Activate Previous Pane"),
            "previous",
        );
        assert_eq!(matched.priority_tier(), 0);
    }

    /// An empty query must not promote every action into the higher tier.
    #[test]
    fn empty_query_does_not_promote() {
        let matched = MatchedBinding::new_with_query(
            FuzzyMatchResult::no_match(),
            binding("Activate Previous Pane"),
            "   ",
        );
        assert_eq!(matched.priority_tier(), 0);
    }
}
