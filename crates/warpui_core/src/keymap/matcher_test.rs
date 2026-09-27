use crate::keymap::ContextPredicate;
use crate::keymap::macros::*;

use super::*;

#[test]
fn test_matcher() -> anyhow::Result<()> {
    #[derive(Debug, PartialEq)]
    enum Action {
        A(String),
        B,
        AB,
    }

    let keymap = Keymap::new(vec![
        FixedBinding::new("a", Action::A("b".into()), id!("a")),
        FixedBinding::new("b", Action::B, id!("a")),
        FixedBinding::new("a b", Action::AB, id!("a") | id!("b")),
    ]);

    let mut ctx_a = Context::default();
    ctx_a.set.insert("a");

    let mut ctx_b = Context::default();
    ctx_b.set.insert("b");

    let mut matcher = Matcher::new(keymap);

    let view_id = EntityId::new();

    // Basic match
    assert_eq!(
        matcher
            .test_keystroke("a", view_id, &ctx_a)
            .unwrap()
            .as_action::<Action>(),
        &Action::A("b".into())
    );

    // Multi-keystroke match
    assert!(matcher.test_keystroke("a", view_id, &ctx_b).is_none());
    assert_eq!(
        matcher
            .test_keystroke("b", view_id, &ctx_b)
            .unwrap()
            .as_action::<Action>(),
        &Action::AB
    );

    // Failed matches don't interfere with matching subsequent keys
    assert!(matcher.test_keystroke("x", view_id, &ctx_a).is_none());
    assert_eq!(
        matcher
            .test_keystroke("a", view_id, &ctx_a)
            .unwrap()
            .as_action::<Action>(),
        &Action::A("b".into())
    );

    // Pending keystrokes are cleared when the context changes
    assert!(matcher.test_keystroke("a", view_id, &ctx_b).is_none());
    assert_eq!(
        matcher
            .test_keystroke("b", view_id, &ctx_a)
            .unwrap()
            .as_action::<Action>(),
        &Action::B
    );

    let mut ctx_c = Context::default();
    ctx_c.set.insert("c");

    // Pending keystrokes are maintained per-view
    let view_id1 = EntityId::new();
    let view_id2 = EntityId::new();
    assert_ne!(view_id1, view_id2);
    assert!(matcher.test_keystroke("a", view_id1, &ctx_b).is_none());
    assert!(matcher.test_keystroke("a", view_id2, &ctx_c).is_none());
    assert_eq!(
        matcher
            .test_keystroke("b", view_id1, &ctx_b)
            .unwrap()
            .as_action::<Action>(),
        &Action::AB
    );

    Ok(())
}

#[test]
fn test_editable_binding_matching() {
    #[derive(Debug, PartialEq)]
    enum Action {
        A(&'static str),
        B,
        AOrB,
    }

    let mut keymap = Keymap::default();
    use crate::keymap::macros::*;
    keymap.register_editable_bindings([
        EditableBinding::new("a", "Action for A", Action::A("b"))
            .with_key_binding("a")
            .with_context_predicate(id!("a")),
        EditableBinding::new("b", "Action for B", Action::B)
            .with_key_binding("b")
            .with_context_predicate(id!("a")),
        EditableBinding::new("a_or_b", "Action for A or B", Action::AOrB)
            .with_key_binding("a b")
            .with_context_predicate(id!("a") | id!("b")),
    ]);

    let mut ctx_a = Context::default();
    ctx_a.set.insert("a");

    let mut ctx_b = Context::default();
    ctx_b.set.insert("b");

    let mut matcher = Matcher::new(keymap);

    let view_id = EntityId::new();

    // Basic match
    assert_eq!(
        matcher
            .test_keystroke("a", view_id, &ctx_a)
            .unwrap()
            .as_action::<Action>(),
        &Action::A("b"),
    );

    // Multi-keystroke match
    assert!(matcher.test_keystroke("a", view_id, &ctx_b).is_none());
    assert_eq!(
        matcher
            .test_keystroke("b", view_id, &ctx_b)
            .unwrap()
            .as_action::<Action>(),
        &Action::AOrB
    );

    // Failed matches don't interfere with matching subsequent keys
    assert!(matcher.test_keystroke("x", view_id, &ctx_a).is_none());
    assert_eq!(
        matcher
            .test_keystroke("a", view_id, &ctx_a)
            .unwrap()
            .as_action::<Action>(),
        &Action::A("b")
    );

    // Pending keystrokes are cleared when the context changes
    assert!(matcher.test_keystroke("a", view_id, &ctx_b).is_none());
    assert_eq!(
        matcher
            .test_keystroke("b", view_id, &ctx_a)
            .unwrap()
            .as_action::<Action>(),
        &Action::B
    );

    let mut ctx_c = Context::default();
    ctx_c.set.insert("c");

    // Pending keystrokes are maintained per-view
    let view_id1 = EntityId::new();
    let view_id2 = EntityId::new();
    assert_ne!(view_id1, view_id2);
    assert!(matcher.test_keystroke("a", view_id1, &ctx_b).is_none());
    assert!(matcher.test_keystroke("a", view_id2, &ctx_c).is_none());
    assert_eq!(
        matcher
            .test_keystroke("b", view_id1, &ctx_b)
            .unwrap()
            .as_action::<Action>(),
        &Action::AOrB
    );
}

#[test]
fn test_bindings_for_context() {
    #[derive(Debug)]
    enum Action {
        A,
        B,
        C,
    }
    let keymap = Keymap::new(vec![
        FixedBinding::new("a", Action::A, id!("a")),
        FixedBinding::new("b", Action::B, id!("b")),
        FixedBinding::new("c", Action::C, id!("b")),
    ]);
    let matcher = Matcher::new(keymap);

    let mut ctx_a = Context::default();
    ctx_a.set.insert("a");

    let mut ctx_b = Context::default();
    ctx_b.set.insert("b");

    // Getting bindings for the 'a' context returns a single result
    let ctx_a_bindings = matcher
        .bindings_for_context(ctx_a)
        .filter_map(|bind| match bind.trigger {
            Trigger::Keystrokes(keys) => {
                assert_eq!(keys.len(), 1);
                Some(keys[0].normalized())
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(ctx_a_bindings.len(), 1);
    assert_eq!(ctx_a_bindings, vec!["a"]);

    // Getting bindings for the 'b' context returns two results, in the reverse order they
    // added, so the "c" binding first followed by the "b" binding
    let ctx_b_bindings = matcher
        .bindings_for_context(ctx_b)
        .filter_map(|bind| match bind.trigger {
            Trigger::Keystrokes(keys) => {
                assert_eq!(keys.len(), 1);
                Some(keys[0].normalized())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(ctx_b_bindings, vec!["c", "b"]);
}

/// Regression test for the defect shape fixed by `90c2484d`
/// (`app/src/workspace/mod.rs`'s redundant `FixedBinding::custom(CustomAction::
/// ToggleProjectExplorer, ...)`, registered alongside an `EditableBinding` exposing the very
/// same custom tag): once a `custom_tag_to_keystroke` fn is installed,
/// `register_fixed_bindings`/`register_editable_bindings` each independently convert any
/// `Trigger::Custom(tag)` they see into the *same* `Trigger::Keystrokes`, at registration time,
/// storing the result on that binding alone. Settings > Keyboard Shortcuts can only clear or
/// remap an *editable* binding, so a fixed binding sharing the tag keeps its own converted
/// keystroke forever -- the user's "removal" of the shortcut is invisible to it.
#[test]
fn shadowing_fixed_binding_outlives_clearing_the_editable_one() {
    #[derive(Debug, PartialEq)]
    enum Action {
        ToggleExplorer,
    }

    const TOGGLE_EXPLORER_TAG: CustomTag = 1;
    const BINDING_NAME: &str = "workspace:left_panel_project_explorer";
    let to_keystroke =
        |tag: CustomTag| (tag == TOGGLE_EXPLORER_TAG).then(|| Keystroke::parse("ctrl-2").unwrap());

    let mut ctx = Context::default();
    ctx.set.insert("Workspace");
    let view_id = EntityId::new();

    // Shape 1: the pre-fix registration -- both a FixedBinding (the macOS menu's hidden
    // keyboard-equivalent hint) and an EditableBinding (what Settings actually shows) carry
    // `Trigger::Custom(TOGGLE_EXPLORER_TAG)`.
    let mut shadowed = Matcher::new(Keymap::default());
    shadowed.convert_custom_triggers_to_keystroke_triggers(to_keystroke);
    shadowed.register_fixed_bindings([FixedBinding::custom(
        TOGGLE_EXPLORER_TAG,
        Action::ToggleExplorer,
        "Toggle project explorer",
        id!("Workspace"),
    )]);
    shadowed.register_editable_bindings([EditableBinding::new(
        BINDING_NAME,
        "Left Panel: Project explorer",
        Action::ToggleExplorer,
    )
    .with_context_predicate(id!("Workspace"))
    .with_custom_action(TOGGLE_EXPLORER_TAG)]);

    assert!(
        shadowed.test_keystroke("ctrl-2", view_id, &ctx).is_some(),
        "setup sanity check: the keystroke should work before any remap"
    );

    // The user clears the binding from Settings -- the only one they can see or edit.
    shadowed.set_custom_trigger(BINDING_NAME.to_owned(), Trigger::Empty);
    assert!(
        shadowed.test_keystroke("ctrl-2", view_id, &ctx).is_some(),
        "defect: a FixedBinding sharing a custom tag with an EditableBinding keeps firing \
         even after the EditableBinding is cleared, because the FixedBinding's own \
         converted keystroke was never touched"
    );

    // Shape 2: the fix -- only the EditableBinding is registered (`90c2484d` deletes the
    // FixedBinding entirely). Clearing it now actually clears the shortcut.
    let mut fixed = Matcher::new(Keymap::default());
    fixed.convert_custom_triggers_to_keystroke_triggers(to_keystroke);
    fixed.register_editable_bindings([EditableBinding::new(
        BINDING_NAME,
        "Left Panel: Project explorer",
        Action::ToggleExplorer,
    )
    .with_context_predicate(id!("Workspace"))
    .with_custom_action(TOGGLE_EXPLORER_TAG)]);

    assert!(fixed.test_keystroke("ctrl-2", view_id, &ctx).is_some());
    fixed.set_custom_trigger(BINDING_NAME.to_owned(), Trigger::Empty);
    assert!(
        fixed.test_keystroke("ctrl-2", view_id, &ctx).is_none(),
        "with no shadowing FixedBinding, clearing the EditableBinding actually clears the \
         shortcut"
    );
}

impl Matcher {
    fn test_keystroke(
        &mut self,
        keystroke: &str,
        view_id: EntityId,
        ctx: &Context,
    ) -> Option<Arc<dyn Action>> {
        match self.push_keystroke(Keystroke::parse(keystroke).unwrap(), view_id, ctx) {
            MatchResult::Action(action) => Some(action),
            _ => None,
        }
    }
}

trait AsAction {
    fn as_action<A: Action>(&self) -> &A;
}

impl AsAction for Arc<dyn Action> {
    fn as_action<A: Action>(&self) -> &A {
        self.as_ref().as_any().downcast_ref::<A>().unwrap()
    }
}

// `debug_assertions` because `validate_bindings` is a no-op in release.

#[cfg(debug_assertions)]
#[derive(Debug, PartialEq)]
enum ValidationTestAction {
    ReopenClosedSession,
}

/// Mirrors `warp_tui::keybindings::is_tui_owned_binding`: a keystroke binding
/// that matches a TUI view context must be TUI-owned (a `tui:`-prefixed name or
/// the `tui` group). Non-keystroke triggers are exempt.
#[cfg(debug_assertions)]
fn require_tui_owned(binding: BindingLens) -> IsBindingValid {
    if !matches!(binding.trigger, Trigger::Keystrokes(_)) {
        return IsBindingValid::Yes;
    }
    if binding.name.starts_with("tui:") || binding.group == Some("tui") {
        IsBindingValid::Yes
    } else {
        IsBindingValid::No
    }
}

#[cfg(debug_assertions)]
fn matcher_with_reopen_binding(context_predicate: Option<ContextPredicate>) -> Matcher {
    let mut binding = EditableBinding::new(
        "app:reopen_closed_session",
        "Reopen closed session",
        ValidationTestAction::ReopenClosedSession,
    )
    .with_key_binding("ctrl-alt-t");
    if let Some(predicate) = context_predicate {
        binding = binding.with_context_predicate(predicate);
    }

    let mut keymap = Keymap::default();
    keymap.register_editable_bindings([binding]);

    let mut matcher = Matcher::new(keymap);
    // Validate against a TUI view context, as the headless TUI does.
    let mut tui_context = Context::default();
    tui_context.set.insert("RootTuiView");
    matcher.register_binding_validator(tui_context, require_tui_owned);
    matcher
}

/// Without a context predicate the binding defaults to `Just(true)`, so its
/// keystroke matches the TUI view context and, not being TUI-owned, fails
/// validation — reproducing the debug-TUI startup panic.
#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "Bindings failed validation")]
fn unscoped_gui_keystroke_binding_fails_tui_validation() {
    let mut matcher = matcher_with_reopen_binding(None);
    matcher.validate_bindings();
}

/// Scoping the binding to the GUI `Workspace` context (the fix) keeps it out of
/// TUI keymap contexts, so validation passes.
#[cfg(debug_assertions)]
#[test]
fn workspace_scoped_binding_passes_tui_validation() {
    let mut matcher = matcher_with_reopen_binding(Some(id!("Workspace")));
    matcher.validate_bindings();
}
