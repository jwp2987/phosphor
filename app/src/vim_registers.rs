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

    /// Blocking read, for the one caller without a key hold (the code editor's vim paste). The
    /// platform read is bounded by its own first-answer deadline.
    pub fn read_from_register(
        &self,
        register_name: char,
        ctx: &mut ModelContext<Self>,
    ) -> Option<RegisterContent> {
        // If this is coming from the system clipboard, we need to check if the content was yanked
        // using a linewise motion. If it was, it needs to be pasted on its own line.
        if Self::points_to_system_clipboard(register_name, ctx) {
            let register_content = self.registers.get(&'+');
            // The ONE remaining blocking clipboard read in the editor (#1380, #1405): the
            // register's value decides the edit this very keystroke, and the modal editors have
            // no key hold to keep the keys typed behind an asynchronous read in order. Convert it
            // to `request_read` (the vim action completing when the answer lands) as soon as the
            // input queue can hold the keys typed meanwhile. Text only, so the HTML and image
            // probes (the slow part of a stalling owner) are skipped; the platform read is itself
            // bounded (first-answer deadline, blocked-reader cap, size cap).
            let clipboard_content = warpui::clipboard::read_content_blocking(
                ctx.clipboard(),
                warpui::clipboard::ReadKind::TextAndPaths,
            )
            .plain_text;

            // Read this model's register entry for the system clipboard. It may be different if
            // the user wrote to the clipboard from a different app. If it's the same, we will
            // assume that we wrote this entry to the clipboard, and we return our own stored value
            // for the motion type.
            if register_content.is_some_and(|register| register.text == clipboard_content) {
                register_content.cloned()
            } else {
                // If the system clipboard content doesn't match, assume it came from a different
                // app and therefore could not have been copied from a Vim linewise motion. So,
                // treat it as charwise.
                Some(RegisterContent {
                    text: clipboard_content,
                    motion_type: MotionType::Charwise,
                })
            }
        } else {
            self.registers.get(&register_name).cloned()
        }
    }
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
