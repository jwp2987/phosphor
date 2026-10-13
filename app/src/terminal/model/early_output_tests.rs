use crate::features::FeatureFlag;
use crate::terminal::event::Event as TerminalEvent;
use crate::terminal::model::block::TranscriptScope;
use crate::terminal::{
    event_listener::ChannelEventListener,
    model::{
        ansi::{self, Handler},
        blocks::BlockList,
        session::SessionInfo,
        test_utils::TestBlockListBuilder,
    },
    shell::ShellType,
};

use super::TypeaheadMode;

/// Create a new bootstrapped block list that will use the set typeahead mode.
fn new_block_list(event_proxy: ChannelEventListener, mode: TypeaheadMode) -> BlockList {
    let mut block_list = TestBlockListBuilder::new()
        .with_channel_event_proxy(event_proxy)
        .build();

    let (shell, shell_version) = match mode {
        TypeaheadMode::ShellReported => ("zsh", "5.0"),
        TypeaheadMode::InputMatching => ("bash", "3.2"),
    };
    let init_shell_value = ansi::InitShellValue {
        shell: shell.into(),
        ..Default::default()
    };

    let bootstrapped_value = ansi::BootstrappedValue {
        shell: shell.into(),
        shell_version: Some(shell_version.into()),
        ..Default::default()
    };
    let session_info = SessionInfo::create_pending(
        ShellType::from_name(shell).unwrap(),
        init_shell_value,
        None,
        None,
        None,
        false,
        None,
    )
    .merge_from_bootstrapped_value(bootstrapped_value.clone(), false);

    block_list.bootstrapped(bootstrapped_value);
    block_list.early_output_mut().init_session(&session_info);
    assert_eq!(block_list.early_output_mut().mode, mode);

    block_list.command_finished(Default::default());
    block_list.prompt_only_precmd(Default::default());
    assert!(block_list.is_bootstrapping_precmd_done());
    block_list
}

#[test]
fn test_lazy_background_insertion() {
    let mut block_list = new_block_list(
        ChannelEventListener::new_for_test(),
        TypeaheadMode::ShellReported,
    );

    // Mimic the shell resetting terminal styles between commands.
    block_list.carriage_return();
    block_list.clear_line(ansi::LineClearMode::Right);
    block_list.terminal_attribute(ansi::Attr::Reset);

    // At this point, the background block should not have been inserted.
    assert!(block_list.background_block_mut().is_none());
    assert!(block_list.is_empty());

    // Write actual background output.
    block_list.input('h');
    block_list.input('i');
    block_list.linefeed();
    block_list.on_finish_byte_processing(&ansi::ProcessorInput::new(&[]));

    assert!(!block_list.is_empty());
    let background = block_list
        .background_block_mut()
        .expect("Background block should exist");
    assert_eq!(background.output_to_string(), "hi\n");
}

#[test]
fn test_background_triggers_wakeup() {
    let (wakeups_tx, wakeups_rx) = async_channel::unbounded();
    let mut block_list = new_block_list(
        ChannelEventListener::builder_for_test()
            .with_wakeups_tx(wakeups_tx)
            .build(),
        TypeaheadMode::ShellReported,
    );
    while !wakeups_rx.is_empty() {
        let _ = wakeups_rx.try_recv();
    }

    // Write background output.
    block_list.input('b');

    // There should now be a background block and a wakeup call.
    assert!(block_list.background_block_mut().is_some());
    assert!(wakeups_rx.recv_blocking().is_ok());
}

#[test]
fn test_queued_typeahead_input_matching() {
    let mut block_list = new_block_list(
        ChannelEventListener::new_for_test(),
        TypeaheadMode::InputMatching,
    );

    // Provide two lines of typeahead.
    block_list
        .early_output_mut()
        .push_user_input("first\rsecond");

    // Mimic the shell echoing and executing the first command.
    block_list.input('f');
    block_list.input('i');
    block_list.input('r');
    block_list.input('s');
    block_list.input('t');
    block_list.carriage_return();
    block_list.linefeed();
    assert!(block_list.active_block().is_command_empty());
    // With input matching, typeahead is never written to the background block.
    assert!(block_list.background_block_mut().is_none());
    // On preexec, the block list detects that the command is missing and restores
    // it from typeahead.
    block_list.preexec(ansi::PreexecValue {
        command: "first".into(),
        session_id: None,
    });
    assert_eq!(block_list.active_block().command_to_string(), "first");
    block_list.command_finished(Default::default());
    block_list.prompt_only_precmd(Default::default());

    // Once the second line of typeahead is echoed, it should be recognized as typeahead.
    block_list.input('s');
    block_list.input('e');
    block_list.input('c');
    block_list.input('o');
    block_list.input('n');
    block_list.input('d');
    assert_eq!(block_list.early_output().typeahead(), "second");
}

#[test]
fn test_queued_typeahead_shell_reported() {
    let mut block_list = new_block_list(
        ChannelEventListener::new_for_test(),
        TypeaheadMode::ShellReported,
    );

    // Mimic the shell echoing and executing the first command.
    block_list.input('f');
    block_list.input('i');
    block_list.input('r');
    block_list.input('s');
    block_list.input('t');
    block_list.carriage_return();
    block_list.linefeed();
    block_list.on_finish_byte_processing(&ansi::ProcessorInput::new(&[]));
    assert!(block_list.active_block().is_command_empty());
    assert_eq!(
        block_list
            .background_block_mut()
            .expect("Background block should exist")
            .output_to_string(),
        "first\n"
    );
    // On preexec, the block list detects that the command is missing and restores
    // it from background output, removing the background block in the process.
    block_list.preexec(ansi::PreexecValue {
        command: "first".into(),
        session_id: None,
    });
    assert_eq!(block_list.active_block().command_to_string(), "first");
    assert!(block_list.background_block_mut().is_none());

    block_list.command_finished(Default::default());
    block_list.prompt_only_precmd(Default::default());

    // Now, when the second line is echoed, it should be recognized as typeahead.
    block_list.input('s');
    block_list.input('e');
    block_list.input('c');
    block_list.input('o');
    block_list.input('n');
    block_list.input('d');
    // Mimic the ESC-i keybinding, which clears the input buffer.
    block_list.input_buffer(ansi::InputBufferValue {
        buffer: "second".into(),
        session_id: None,
    });
    // zsh appears to use `\r\e[J` (carriage return and clear from cursor to end of screen)
    // to clear the line. There are lots of ways of doing this, and it doesn't
    // matter exactly which one the shell uses as long as the effect on the grid is the same.
    block_list.carriage_return();
    block_list.clear_screen(ansi::ClearMode::Below);
    block_list.on_finish_byte_processing(&ansi::ProcessorInput::new(&[]));

    assert_eq!(block_list.early_output().typeahead(), "second");
    // Unlike regular blocks, if the output grid of a background block is cleared
    // then it becomes hidden.
    assert!(block_list
        .background_block_mut()
        .expect("Block should exist")
        .is_empty(&TranscriptScope::Terminal));
}

fn report(block_list: &mut BlockList, buffer: &str) {
    block_list.input_buffer(ansi::InputBufferValue {
        buffer: buffer.into(),
        session_id: None,
    });
}

/// Without `FeatureFlag::TypeaheadHandoff`, a second report replaces the first, as at the pin.
#[test]
fn second_report_replaces_the_first_without_the_typeahead_handoff() {
    let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(false);
    let mut block_list = new_block_list(
        ChannelEventListener::new_for_test(),
        TypeaheadMode::ShellReported,
    );
    report(&mut block_list, "first");
    report(&mut block_list, "second");
    assert_eq!(block_list.early_output().typeahead(), "second");

    // And a precmd clears a report the view has not inserted yet.
    block_list.prompt_only_precmd(Default::default());
    assert_eq!(block_list.early_output().typeahead(), "");
}

/// The shell clears its line when it reports it, so two reports hold different text and the
/// first must survive the second.
#[test]
fn typeahead_handoff_appends_a_second_report() {
    let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
    let mut block_list = new_block_list(
        ChannelEventListener::new_for_test(),
        TypeaheadMode::ShellReported,
    );
    report(&mut block_list, "echo 47");
    report(&mut block_list, " >> ~/burst");
    assert_eq!(block_list.early_output().typeahead(), "echo 47 >> ~/burst");

    // The view replaces what it already inserted with the whole accumulated report.
    let (typeahead, inserted) = block_list
        .early_output_mut()
        .advance_typeahead()
        .expect("typeahead was reported");
    assert_eq!(typeahead, "echo 47 >> ~/burst");
    assert_eq!(inserted.as_usize(), 0);
}

#[test]
fn typeahead_handoff_precmd_keeps_a_report_the_editor_has_not_inserted() {
    let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
    let mut block_list = new_block_list(
        ChannelEventListener::new_for_test(),
        TypeaheadMode::ShellReported,
    );
    report(&mut block_list, "pwd");
    block_list.prompt_only_precmd(Default::default());
    assert_eq!(block_list.early_output().typeahead(), "pwd");
    assert!(block_list.early_output().has_uninserted_typeahead());

    // Once inserted, the next precmd clears it as before.
    block_list.early_output_mut().advance_typeahead();
    assert!(!block_list.early_output().has_uninserted_typeahead());
    block_list.prompt_only_precmd(Default::default());
    assert_eq!(block_list.early_output().typeahead(), "");
}

#[test]
fn clearing_typeahead_for_a_new_command_restarts_insertion() {
    let mut block_list = new_block_list(
        ChannelEventListener::new_for_test(),
        TypeaheadMode::ShellReported,
    );
    report(&mut block_list, "ls");
    block_list.early_output_mut().advance_typeahead();
    block_list
        .early_output_mut()
        .clear_typeahead_for_new_command();
    report(&mut block_list, "pwd");
    let (typeahead, inserted) = block_list
        .early_output_mut()
        .advance_typeahead()
        .expect("typeahead was reported");
    assert_eq!(typeahead, "pwd");
    assert_eq!(
        inserted.as_usize(),
        0,
        "a report after the command was submitted must not overwrite the fresh editor's text"
    );
}

/// The hand-off waits on the report it requested; an empty line answers it too.
#[test]
fn typeahead_handoff_hears_about_an_empty_report() {
    let (events_tx, events_rx) = async_channel::unbounded();
    let mut block_list = {
        let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(false);
        new_block_list(
            ChannelEventListener::builder_for_test()
                .with_terminal_events_tx(events_tx)
                .build(),
            TypeaheadMode::ShellReported,
        )
    };
    let typeahead_events = |events_rx: &async_channel::Receiver<TerminalEvent>| {
        std::iter::from_fn(|| events_rx.try_recv().ok())
            .filter(|event| matches!(event, TerminalEvent::Typeahead))
            .count()
    };
    typeahead_events(&events_rx);

    {
        let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(false);
        report(&mut block_list, "");
        assert_eq!(typeahead_events(&events_rx), 0);
    }

    let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
    report(&mut block_list, "");
    assert_eq!(typeahead_events(&events_rx), 1);
    assert_eq!(block_list.early_output().typeahead(), "");
}
