use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use ai::skills::SkillProvider;
use warp_core::ui::appearance::Appearance;
use warp_core::ui::theme::color::internal_colors;
use warpui::{
    elements::{
        Border, ChildView, Clipped, ClippedScrollStateHandle, ClippedScrollable, ConstrainedBox,
        Container, CornerRadius, CrossAxisAlignment, Element, Fill as ElementFill, Flex, Hoverable,
        MainAxisSize, MouseStateHandle, Padding, ParentElement, Radius, SavePosition, ScrollTarget,
        ScrollToPositionMode, ScrollbarWidth, Shrinkable, Text,
    },
    platform::Cursor,
    text_layout::ClipConfig,
    AppContext, Entity, SingletonEntity, TypedActionView, View, ViewContext, ViewHandle,
};

use crate::ai::skills::{
    SkillInventoryDuplicate, SkillInventoryItem, SkillManager, SkillManagerEvent,
};
use crate::editor::{
    EditorOptions, EditorView, Event as EditorEvent, PropagateAndNoOpNavigationKeys,
    PropagateHorizontalNavigationKeys, TextOptions,
};
use crate::view_components::dropdown::{Dropdown, DropdownItem};

const PANEL_PADDING: f32 = 8.0;
const ROW_PADDING_VERTICAL: f32 = 5.0;
const ROW_PADDING_HORIZONTAL: f32 = 8.0;
// filter tab: keep a compact height, using a 12px font size + a
// high-contrast active state to hold up readability, avoiding having click
// events intercepted by the outer ClippedScrollable shell.
const FILTER_BUTTON_HEIGHT: f32 = 24.0;
const FILTER_BUTTON_HORIZONTAL_PADDING: f32 = 8.0;
const FILTER_BUTTON_CORNER_RADIUS: f32 = 4.0;
// The maximum number of source tabs shown inline; beyond this it collapses
// into "All + a source dropdown". Chosen as 4: at a typical sidebar width,
// "All" plus about 4 short source names still fit inline. The layout engine
// is a single-pass retained tree that can't adaptively collapse based on
// measured pixel width, so the source count is used as the collapse threshold.
const FILTER_TABS_INLINE_MAX: usize = 4;

#[derive(Clone, Debug)]
pub enum SkillManagerPanelAction {
    SelectProviderFilter(Option<SkillProvider>),
    EditSkill(PathBuf),
}

#[derive(Clone, Debug)]
pub enum SkillManagerPanelEvent {
    OpenSkillFile { path: PathBuf },
}

pub struct SkillManagerPanel {
    selected_path: Option<PathBuf>,
    provider_filter: Option<SkillProvider>,
    query_editor: ViewHandle<EditorView>,
    // The "Source" dropdown that holds each source tab in the collapsed state (source count > FILTER_TABS_INLINE_MAX).
    source_dropdown: ViewHandle<Dropdown<SkillManagerPanelAction>>,
    filter_mouse_states: RefCell<HashMap<Option<SkillProvider>, MouseStateHandle>>,
    row_mouse_states: RefCell<HashMap<PathBuf, MouseStateHandle>>,
    list_scroll_state: ClippedScrollStateHandle,
}

impl SkillManagerPanel {
    pub fn new(ctx: &mut ViewContext<Self>) -> Self {
        let query_editor = ctx.add_typed_action_view(|ctx| {
            let options = EditorOptions {
                text: TextOptions::ui_text(Some(Appearance::as_ref(ctx).ui_font_subheading()), Appearance::as_ref(ctx)),
                propagate_and_no_op_vertical_navigation_keys:
                    PropagateAndNoOpNavigationKeys::AtBoundary,
                propagate_horizontal_navigation_keys: PropagateHorizontalNavigationKeys::Always,
                single_line: true,
                clear_selections_on_blur: true,
                convert_newline_to_space: true,
                ..Default::default()
            };
            let mut editor = EditorView::new(options, ctx);
            editor.set_placeholder_text(crate::t!("skill-manager-search-placeholder"), ctx);
            editor
        });

        ctx.subscribe_to_view(&query_editor, |me, _, event, ctx| {
            if matches!(
                event,
                EditorEvent::Edited(_)
                    | EditorEvent::BufferReplaced
                    | EditorEvent::BufferReinitialized
            ) {
                me.scroll_selected_path_into_view_with_ctx(ctx);
                ctx.notify();
            }
        });

        ctx.subscribe_to_model(&Appearance::handle(ctx), |_, _, _, ctx| {
            ctx.notify();
        });

        let skill_manager = SkillManager::handle(ctx);
        ctx.subscribe_to_model(&skill_manager, |me, _manager, event, ctx| match event {
            SkillManagerEvent::InventoryChanged => {
                let inventory = SkillManager::as_ref(ctx).list_skill_inventory(ctx);
                if me.provider_filter.is_some_and(|provider| {
                    !Self::providers_in_inventory(&inventory).contains(&provider)
                }) {
                    me.provider_filter = None;
                }
                let query = me.query(ctx);
                let items = Self::filter_inventory(&inventory, &query, me.provider_filter);
                me.scroll_selected_path_into_view(&items);
                // The set of sources changes with the inventory; rebuild the collapsed dropdown's items and selection.
                me.rebuild_source_dropdown(ctx);
                ctx.notify();
            }
        });

        // The "Source" dropdown used in the collapsed state. Created by this
        // panel's ViewContext; the SelectProviderFilter it dispatches bubbles
        // back up to this panel's handle_action.
        let source_dropdown = ctx.add_typed_action_view(|ctx| {
            let mut dropdown = Dropdown::new(ctx);
            // The dropdown header hugs the text width, sitting side by side
            // with the "All" button on the left, without stretching to fill the row.
            dropdown.set_main_axis_size(MainAxisSize::Min, ctx);
            // When the "All" item is selected, the closed state shows the
            // "Source" placeholder, to avoid duplicating the separate "All" button on the left.
            let all_label = crate::t!("skill-manager-filter-all").to_string();
            let source_label = crate::t!("skill-manager-filter-provider").to_string();
            dropdown.set_menu_header_text_override(move |label| {
                if label == all_label {
                    source_label.clone()
                } else {
                    label.to_string()
                }
            });
            dropdown
        });

        let mut me = Self {
            selected_path: None,
            provider_filter: None,
            query_editor,
            source_dropdown,
            filter_mouse_states: RefCell::new(HashMap::new()),
            row_mouse_states: RefCell::new(HashMap::new()),
            list_scroll_state: ClippedScrollStateHandle::default(),
        };
        me.rebuild_source_dropdown(ctx);
        me
    }

    /// Rebuilds the "Source" dropdown's items and selection.
    ///
    /// The first item, "All", corresponds to clearing the filter (index 0);
    /// the remaining items correspond in order to each provider present in
    /// the current inventory. The selection is driven by `provider_filter`,
    /// staying consistent with the "All" button on the left.
    fn rebuild_source_dropdown(&mut self, ctx: &mut ViewContext<Self>) {
        let inventory = SkillManager::as_ref(ctx).list_skill_inventory(ctx);
        let providers = Self::providers_in_inventory(&inventory);

        let mut items = vec![DropdownItem::new(
            crate::t!("skill-manager-filter-all"),
            SkillManagerPanelAction::SelectProviderFilter(None),
        )];
        for provider in &providers {
            items.push(DropdownItem::new(
                provider.to_string(),
                SkillManagerPanelAction::SelectProviderFilter(Some(*provider)),
            ));
        }

        // The first item, "All", occupies index 0, so a matched provider needs +1.
        let selected_index = match self.provider_filter {
            None => 0,
            Some(filter) => providers
                .iter()
                .position(|provider| *provider == filter)
                .map_or(0, |position| position + 1),
        };

        self.source_dropdown.update(ctx, |dropdown, ctx| {
            dropdown.set_items(items, ctx);
            dropdown.set_selected_by_index(selected_index, ctx);
        });
    }

    fn query(&self, app: &AppContext) -> String {
        self.query_editor
            .as_ref(app)
            .buffer_text(app)
            .trim()
            .to_lowercase()
    }

    fn providers_in_inventory(inventory: &[SkillInventoryItem]) -> Vec<SkillProvider> {
        let mut providers = inventory
            .iter()
            .flat_map(|item| item.duplicates.iter().map(|duplicate| duplicate.provider))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        providers.sort_by_key(|provider| provider.to_string());
        providers
    }

    fn filter_inventory(
        inventory: &[SkillInventoryItem],
        query: &str,
        provider_filter: Option<SkillProvider>,
    ) -> Vec<SkillInventoryItem> {
        inventory
            .iter()
            .filter_map(|item| {
                let duplicates = item
                    .duplicates
                    .iter()
                    .filter(|duplicate| {
                        provider_filter.is_none_or(|provider| duplicate.provider == provider)
                            && (query.is_empty()
                                || duplicate.name.to_lowercase().contains(query)
                                || duplicate.description.to_lowercase().contains(query)
                                || duplicate
                                    .path
                                    .display()
                                    .to_string()
                                    .to_lowercase()
                                    .contains(query))
                    })
                    .cloned()
                    .collect::<Vec<_>>();

                let default_skill = duplicates.first()?.clone();
                Some(SkillInventoryItem {
                    name: item.name.clone(),
                    default_skill,
                    duplicates,
                })
            })
            .collect()
    }

    fn selected_path_is_visible(path: &Path, items: &[SkillInventoryItem]) -> bool {
        items
            .iter()
            .flat_map(|item| item.duplicates.iter())
            .any(|duplicate| duplicate.path.as_path() == path)
    }

    fn skill_row_position_id(path: &Path) -> String {
        format!("skill-manager-row:{}", path.to_string_lossy())
    }

    fn scroll_selected_path_into_view(&self, items: &[SkillInventoryItem]) {
        let Some(path) = self.selected_path.as_deref() else {
            return;
        };
        if !Self::selected_path_is_visible(path, items) {
            return;
        }

        self.list_scroll_state.scroll_to_position(ScrollTarget {
            position_id: Self::skill_row_position_id(path),
            mode: ScrollToPositionMode::FullyIntoView,
        });
    }

    fn scroll_selected_path_into_view_with_ctx(&self, ctx: &AppContext) {
        let inventory = SkillManager::as_ref(ctx).list_skill_inventory(ctx);
        let query = self.query(ctx);
        let items = Self::filter_inventory(&inventory, &query, self.provider_filter);
        self.scroll_selected_path_into_view(&items);
    }

    fn filter_mouse_state_for(&self, provider: Option<SkillProvider>) -> MouseStateHandle {
        self.filter_mouse_states
            .borrow_mut()
            .entry(provider)
            .or_default()
            .clone()
    }

    fn row_mouse_state_for(&self, path: &Path) -> MouseStateHandle {
        self.row_mouse_states
            .borrow_mut()
            .entry(path.to_path_buf())
            .or_default()
            .clone()
    }

    fn render_label(
        text: impl Into<String>,
        appearance: &Appearance,
        font_size: f32,
        color: impl Into<pathfinder_color::ColorU>,
    ) -> Box<dyn Element> {
        Text::new_inline(text.into(), appearance.ui_font_family(), font_size)
            .with_color(color.into())
            .with_clip(ClipConfig::ellipsis())
            .finish()
    }

    fn render_filter_button(
        &self,
        label: String,
        is_active: bool,
        provider: Option<SkillProvider>,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let text_color = if is_active {
            theme.main_text_color(theme.background())
        } else {
            theme.sub_text_color(theme.background())
        };
        let state = self.filter_mouse_state_for(provider);
        let action = SkillManagerPanelAction::SelectProviderFilter(provider);

        Hoverable::new(state, move |mouse| {
            // Text is vertically centered within a fixed row height using Flex::row + CrossAxisAlignment::Center.
            let label_row = Flex::row()
                .with_main_axis_size(MainAxisSize::Min)
                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                .with_child(Self::render_label(
                    label.clone(),
                    appearance,
                    appearance.ui_font_body(),
                    text_color,
                ))
                .finish();
            let mut button = Container::new(label_row)
                .with_padding_left(FILTER_BUTTON_HORIZONTAL_PADDING)
                .with_padding_right(FILTER_BUTTON_HORIZONTAL_PADDING)
                .with_corner_radius(CornerRadius::with_all(Radius::Pixels(
                    FILTER_BUTTON_CORNER_RADIUS,
                )));
            if is_active {
                // The active state uses fg_overlay_4 for stronger contrast, so the selection change is visible during quick switching.
                button = button.with_background(internal_colors::fg_overlay_4(theme));
            } else if mouse.is_hovered() {
                button = button.with_background(internal_colors::fg_overlay_2(theme));
            }
            ConstrainedBox::new(button.finish())
                .with_height(FILTER_BUTTON_HEIGHT)
                .finish()
        })
        .with_cursor(Cursor::PointingHand)
        .on_mouse_down(move |ctx, _, _| {
            ctx.dispatch_typed_action(action.clone());
        })
        .finish()
    }

    fn render_filter_rows(
        &self,
        providers: &[SkillProvider],
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        // When there are too many sources, collapse into "All + a source dropdown" to avoid a narrow sidebar clipping the content.
        if providers.len() > FILTER_TABS_INLINE_MAX {
            return self.render_collapsed_filter_rows(appearance);
        }

        let active_filter = self.provider_filter;

        // Use MainAxisSize::Max so the filter tab row fills the panel's width, eliminating whitespace on the right.
        let mut filter_buttons = Flex::row()
            .with_main_axis_size(MainAxisSize::Max)
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_spacing(4.0)
            .with_child(self.render_filter_button(
                crate::t!("skill-manager-filter-all").into(),
                active_filter.is_none(),
                None,
                appearance,
            ));

        for provider in providers {
            filter_buttons.add_child(self.render_filter_button(
                provider.to_string(),
                active_filter == Some(*provider),
                Some(*provider),
                appearance,
            ));
        }

        // No longer wrapped in an outer ClippedScrollable / shell container:
        // 1. A horizontal scroll container would intercept mouse-down for drag
        //    detection, delaying click response when quickly switching tabs.
        // 2. In the inline state, the provider count ≤ FILTER_TABS_INLINE_MAX,
        //    which the sidebar width can accommodate; more sources take the
        //    collapsed-dropdown branch.
        Clipped::new(filter_buttons.finish()).finish()
    }

    /// The collapsed-state filter row: an "All" shortcut button on the left +
    /// a "Source ▾" dropdown on the right.
    ///
    /// Never wrap this in `Clipped`: the dropdown's expanded menu is a
    /// positioned overlay, and clipping would cause it to be lost.
    fn render_collapsed_filter_rows(&self, appearance: &Appearance) -> Box<dyn Element> {
        Flex::row()
            .with_main_axis_size(MainAxisSize::Max)
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_spacing(4.0)
            .with_child(self.render_filter_button(
                crate::t!("skill-manager-filter-all").into(),
                self.provider_filter.is_none(),
                None,
                appearance,
            ))
            .with_child(ChildView::new(&self.source_dropdown).finish())
            .finish()
    }

    fn render_search_input(&self, appearance: &Appearance) -> Box<dyn Element> {
        let theme = appearance.theme();
        let search_row = Shrinkable::new(
            1.0,
            Clipped::new(ChildView::new(&self.query_editor).finish()).finish(),
        )
        .finish();

        Container::new(search_row)
            .with_padding(Padding::uniform(6.0).with_left(12.0).with_right(12.0))
            .with_border(Border::all(1.0).with_border_fill(theme.surface_3()))
            .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.0)))
            .finish()
    }

    fn render_skill_row(
        &self,
        duplicate: &SkillInventoryDuplicate,
        is_selected: bool,
        is_default: bool,
        has_duplicates: bool,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let path = duplicate.path.display().to_string();
        let mut meta = format!("{} · {}", duplicate.provider, duplicate.scope);
        if has_duplicates {
            if is_default {
                meta.push_str(" · ");
                meta.push_str(&crate::t!("skill-manager-meta-default"));
            } else {
                meta.push_str(" · ");
                meta.push_str(&crate::t!("skill-manager-meta-duplicate"));
            }
        }

        let title = Self::render_label(
            duplicate.name.clone(),
            appearance,
            appearance.ui_font_body_large(),
            theme.main_text_color(theme.background()),
        );
        let description = Self::render_label(
            duplicate.description.clone(),
            appearance,
            appearance.ui_font_footnote(),
            theme.sub_text_color(theme.background()),
        );
        let meta = Self::render_label(
            meta,
            appearance,
            appearance.ui_font_footnote(),
            theme.sub_text_color(theme.background()),
        );
        let path = Self::render_label(
            path,
            appearance,
            appearance.ui_font_footnote(),
            theme.sub_text_color(theme.background()),
        );

        let action = SkillManagerPanelAction::EditSkill(duplicate.path.clone());
        let position_id = Self::skill_row_position_id(&duplicate.path);
        let state = self.row_mouse_state_for(&duplicate.path);
        let row = Hoverable::new(state, move |mouse| {
            let background = if is_selected && mouse.is_hovered() {
                Some(internal_colors::fg_overlay_4(theme))
            } else if is_selected {
                Some(internal_colors::fg_overlay_3(theme))
            } else if mouse.is_hovered() {
                Some(internal_colors::fg_overlay_2(theme))
            } else {
                None
            };
            // The outer Flex::row uses MainAxisSize::Max to fill the panel's
            // width, avoiding the Container shrinking to the content's natural
            // width, which would leave whitespace on the right of the highlighted area.
            let content = Flex::row()
                .with_main_axis_size(MainAxisSize::Max)
                .with_cross_axis_alignment(CrossAxisAlignment::Start)
                .with_child(
                    Flex::column()
                        .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
                        .with_spacing(2.0)
                        .with_child(title)
                        .with_child(description)
                        .with_child(meta)
                        .with_child(path)
                        .finish(),
                )
                .finish();
            let mut row = Container::new(content)
                .with_padding_top(ROW_PADDING_VERTICAL)
                .with_padding_bottom(ROW_PADDING_VERTICAL)
                .with_padding_left(ROW_PADDING_HORIZONTAL)
                .with_padding_right(ROW_PADDING_HORIZONTAL)
                .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.0)));
            if let Some(background) = background {
                row = row.with_background(background);
            }
            row.finish()
        })
        .with_cursor(Cursor::PointingHand)
        .on_mouse_down(move |ctx, _, _| {
            ctx.dispatch_typed_action(action.clone());
        })
        .finish();

        SavePosition::new(row, &position_id).finish()
    }

    fn render_skill_list(
        &self,
        items: &[SkillInventoryItem],
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        if items.is_empty() {
            return Container::new(Self::render_label(
                crate::t!("skill-manager-empty"),
                appearance,
                appearance.ui_font_body_large(),
                appearance
                    .theme()
                    .sub_text_color(appearance.theme().background()),
            ))
            .with_uniform_padding(12.0)
            .finish();
        }

        let mut rows = Flex::column()
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_spacing(2.0);
        for item in items {
            let has_duplicates = item.has_duplicates();
            for duplicate in &item.duplicates {
                let is_selected = self
                    .selected_path
                    .as_deref()
                    .is_some_and(|path| path == duplicate.path.as_path());
                let is_default = duplicate.path == item.default_skill.path;
                rows.add_child(self.render_skill_row(
                    duplicate,
                    is_selected,
                    is_default,
                    has_duplicates,
                    appearance,
                ));
            }
        }

        let theme = appearance.theme();
        ClippedScrollable::vertical(
            self.list_scroll_state.clone(),
            rows.finish(),
            ScrollbarWidth::Auto,
            theme.disabled_text_color(theme.background()).into(),
            theme.main_text_color(theme.background()).into(),
            ElementFill::None,
        )
        .with_overlayed_scrollbar()
        .finish()
    }
}

impl TypedActionView for SkillManagerPanel {
    type Action = SkillManagerPanelAction;

    fn handle_action(&mut self, action: &Self::Action, ctx: &mut ViewContext<Self>) {
        match action {
            SkillManagerPanelAction::SelectProviderFilter(provider) => {
                if self.provider_filter == *provider {
                    return;
                }
                self.provider_filter = *provider;
                // Sync the collapsed dropdown's selected item: it isn't aware when the "All" button on the left is clicked.
                self.rebuild_source_dropdown(ctx);
                ctx.notify();
            }
            SkillManagerPanelAction::EditSkill(path) => {
                self.selected_path = Some(path.clone());
                self.scroll_selected_path_into_view_with_ctx(ctx);
                ctx.emit(SkillManagerPanelEvent::OpenSkillFile { path: path.clone() });
                ctx.notify();
            }
        }
    }
}

impl View for SkillManagerPanel {
    fn ui_name() -> &'static str {
        "SkillManagerPanel"
    }

    fn on_focus(&mut self, _focus_ctx: &warpui::FocusContext, ctx: &mut ViewContext<Self>) {
        ctx.focus(&self.query_editor);
        self.scroll_selected_path_into_view_with_ctx(ctx);
        ctx.notify();
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        let inventory = SkillManager::as_ref(app).list_skill_inventory(app);
        let providers = Self::providers_in_inventory(&inventory);
        let query = self.query(app);
        let items = Self::filter_inventory(&inventory, &query, self.provider_filter);

        Container::new(
            Flex::column()
                .with_main_axis_size(MainAxisSize::Max)
                .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
                .with_spacing(8.0)
                .with_child(self.render_search_input(appearance))
                .with_child(self.render_filter_rows(&providers, appearance))
                .with_child(
                    Shrinkable::new(1.0, self.render_skill_list(&items, appearance)).finish(),
                )
                .finish(),
        )
        .with_uniform_padding(PANEL_PADDING)
        .finish()
    }
}

impl Entity for SkillManagerPanel {
    type Event = SkillManagerPanelEvent;
}
