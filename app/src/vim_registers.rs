use std::collections::HashMap;

use vim::vim::MotionType;
use warpui::{AppContext, Entity, ModelContext, SingletonEntity, clipboard::ClipboardContent};

use crate::settings::AppEditorSettings;
use settings::Setting as _;

/// Wraps the actual text that was copied as well as the motion type. That is needed because
/// linewise motions need to be pasted on their own line, NOT at the cursor position.
#[derive(Clone, Debug)]
pub struct RegisterContent {
    pub text: String,
    pub motion_type: MotionType,
}

impl RegisterContent {
    fn new(text: String, motion_type: MotionType) -> Self {
        Self { text, motion_type }
    }
}

#[derive(Default)]
pub struct VimRegisters {
    registers: HashMap<char, RegisterContent>,
}

impl VimRegisters {
    pub fn new() -> Self {
        Self::default()
    }
}

impl VimRegisters {
    /// The registers * and + are special registers that go to the system clipboard.
    fn points_to_system_clipboard(register_name: char, app: &AppContext) -> bool {
        let unnamed_system_clipboard = *AppEditorSettings::as_ref(app)
            .vim_unnamed_system_clipboard
            .value();
        register_name == '*'
            || register_name == '+'
            || (unnamed_system_clipboard && register_name == '"')
    }

    pub fn write_to_register(
        &mut self,
        register_name: char,
        content: String,
        motion_type: MotionType,
        ctx: &mut ModelContext<Self>,
    ) {
        let mut register_name = register_name;
        if Self::points_to_system_clipboard(register_name, ctx) {
            ctx.clipboard()
                .write(ClipboardContent::plain_text(content.clone()));
            // Normalize '*' and '+' to both point to '+'.
            register_name = '+';
        }
        // Even if the text is for the system clipboard, we still need to save a copy of it in this
        // model in order to remember if it was linewise or charwise. That determines how it gets
        // pasted.
        self.registers
            .insert(register_name, RegisterContent::new(content, motion_type));
    }

    /// Starts a read of `register_name` that never blocks the UI thread (#1405). A register of
    /// the editor's own answers at once; one that points to the system clipboard asks the one
    /// async read API and answers when the clipboard does. The caller holds the keys typed
    /// meanwhile behind a gate (the editor's paste hold) and completes the vim action with the
    /// answer.
    pub fn request_register_read(
        &self,
        register_name: char,
        ctx: &mut ModelContext<Self>,
    ) -> RegisterRead {
        if !Self::points_to_system_clipboard(register_name, ctx) {
            return RegisterRead::Ready(self.registers.get(&register_name).cloned());
        }
        let stored = self.registers.get(&'+').cloned();
        #[cfg(test)]
        if let Some(deferred) = DEFERRED_SYSTEM_READ.with(|slot| slot.borrow_mut().take()) {
            return RegisterRead::Pending(Box::pin(async move {
                let text = deferred.await.unwrap_or_default();
                Some(Self::combine_with_stored(stored.as_ref(), text))
            }));
        }
        // Text only, so the HTML and image probes (the slow part of a stalling owner) are skipped.
        let request = warpui::clipboard::request_read(
            ctx.clipboard(),
            warpui::clipboard::ReadKind::TextAndPaths,
        );
        match request.start {
            // The platform reads inline: the answer is known now, the action completes now.
            warpui::clipboard::ReadStart::Ready(outcome) => RegisterRead::Ready(Some(
                Self::combine_with_stored(stored.as_ref(), outcome.into_content().plain_text),
            )),
            warpui::clipboard::ReadStart::Pending(pending) => {
                RegisterRead::Pending(Box::pin(async move {
                    let clipboard_content = pending.await.into_content().plain_text;
                    Some(Self::combine_with_stored(
                        stored.as_ref(),
                        clipboard_content,
                    ))
                }))
            }
        }
    }

    /// Read this model's register entry for the system clipboard. It may be different if the user
    /// wrote to the clipboard from a different app. If it's the same, we assume that we wrote this
    /// entry to the clipboard, and we return our own stored value for the motion type; otherwise
    /// it could not have been copied from a Vim linewise motion, so it is charwise.
    fn combine_with_stored(
        stored: Option<&RegisterContent>,
        clipboard_content: String,
    ) -> RegisterContent {
        if let Some(register) = stored.filter(|register| register.text == clipboard_content) {
            register.clone()
        } else {
            RegisterContent {
                text: clipboard_content,
                motion_type: MotionType::Charwise,
            }
        }
    }

    /// Test-only synchronous form of [`Self::request_register_read`]: the test clipboard reads
    /// inline, so the answer is always ready. Production code has no blocking register read
    /// (#1405); a register that would wait for the clipboard panics here rather than blocking.
    #[cfg(test)]
    pub fn read_from_register(
        &self,
        register_name: char,
        ctx: &mut ModelContext<Self>,
    ) -> Option<RegisterContent> {
        match self.request_register_read(register_name, ctx) {
            RegisterRead::Ready(content) => content,
            RegisterRead::Pending(_) => panic!("register read did not complete inline"),
        }
    }

    /// Makes the next read of a system-clipboard register wait for the returned sender, as a real
    /// clipboard whose owner is slow does: the test controls when (and with what text) it lands.
    #[cfg(test)]
    pub fn defer_next_system_clipboard_read_for_test() -> futures::channel::oneshot::Sender<String>
    {
        let (tx, rx) = futures::channel::oneshot::channel();
        DEFERRED_SYSTEM_READ.with(|slot| *slot.borrow_mut() = Some(rx));
        tx
    }
}

#[cfg(test)]
thread_local! {
    static DEFERRED_SYSTEM_READ: std::cell::RefCell<Option<futures::channel::oneshot::Receiver<String>>> =
        const { std::cell::RefCell::new(None) };
}

/// The answer of [`VimRegisters::request_register_read`].
pub enum RegisterRead {
    /// The register is the editor's own: its content (`None`: empty).
    Ready(Option<RegisterContent>),
    /// The system clipboard is being read.
    Pending(
        std::pin::Pin<
            Box<dyn std::future::Future<Output = Option<RegisterContent>> + Send + 'static>,
        >,
    ),
}

impl Entity for VimRegisters {
    type Event = ();
}

impl SingletonEntity for VimRegisters {}
