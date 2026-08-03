use warpui::App;

use super::InlineMenuModel;
use crate::terminal::input::inline_menu::{InlineMenuAction, InlineMenuType};

#[derive(Clone, Debug, PartialEq, Eq)]
struct TestAction(&'static str);

impl InlineMenuAction for TestAction {
    const MENU_TYPE: InlineMenuType = InlineMenuType::SlashCommands;
}

#[test]
fn refreshed_selection_clears_stale_selected_item() {
    App::test((), |mut app| async move {
        let model = app.add_model(|_| InlineMenuModel::<TestAction>::new());

        model.update(&mut app, |model, ctx| {
            model.update_selected_item(TestAction("enabled"), ctx);
        });
        model.read(&app, |model, _| {
            assert_eq!(model.selected_item(), Some(&TestAction("enabled")));
        });

        // NOTE(adapted): fork's `InlineMenuModel::update_selected_item` takes `A`
        // directly rather than `Option<A>` (Warp's original API). Clearing the
        // selection is done via the separate `clear_selected_item` method instead
        // of passing `None`. The assertion below is unchanged from Warp's original.
        model.update(&mut app, |model, ctx| {
            model.clear_selected_item(ctx);
        });
        model.read(&app, |model, _| {
            assert_eq!(model.selected_item(), None);
        });
    });
}
