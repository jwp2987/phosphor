use std::{collections::HashMap, path::Path};

use itertools::Itertools;
use markdown_parser::{
    FormattedText, FormattedTextFragment, FormattedTextHeader, FormattedTextLine,
};
use warp_core::command::ExitCode;
use warp_core::ui::{appearance::Appearance, color::blend::Blend as _};
use warpui::{
    Element, Entity, SingletonEntity, TypedActionView, View, ViewContext,
    elements::{
        Border, Container, CornerRadius, CrossAxisAlignment, Flex, FormattedTextElement,
        HighlightedHyperlink, MainAxisSize, MouseStateHandle, ParentElement, Radius,
    },
    ui_components::{button::ButtonVariant, components::UiComponent as _},
};

use crate::{
    terminal::ssh::util::InteractiveSshCommand, ui_components::buttons::icon_button,
    ui_components::icons::Icon,
};

pub type FileUploadId = usize;

/// Metadata for a single file upload.
#[derive(Debug, Default)]
struct FileUploadInfo {
    local_file_paths: Vec<String>,
    remote_dest_path: Option<String>,
    remote_host: String,
    remote_port: Option<String>,
    status: FileUploadStatus,
    open_session_button: MouseStateHandle,
    clear_button: MouseStateHandle,
    local_session_open: bool,
    upload_id: FileUploadId,
}

#[derive(Debug, Default)]
enum FileUploadStatus {
    #[default]
    Started,
    AwaitingPassword,
    Completed {
        successful: bool,
    },
}

#[derive(Default)]
pub struct FileUpload {
    uploads: HashMap<FileUploadId, FileUploadInfo>,
    count: usize,
}

pub enum FileUploadEvent {
    CopyFileToRemote {
        command: String,
        upload_id: FileUploadId,
    },
    OpenUploadSession(FileUploadId),
    TerminateUploadSession(FileUploadId),
}

impl Entity for FileUpload {
    type Event = FileUploadEvent;
}

#[derive(Debug)]
pub enum FileUploadAction {
    OpenUploadSession(FileUploadId),
    TerminateUploadSession(FileUploadId),
}

impl TypedActionView for FileUpload {
    type Action = FileUploadAction;

    fn handle_action(&mut self, action: &Self::Action, ctx: &mut ViewContext<Self>) {
        match action {
            FileUploadAction::OpenUploadSession(upload_id) => {
                ctx.emit(FileUploadEvent::OpenUploadSession(*upload_id))
            }
            FileUploadAction::TerminateUploadSession(upload_id) => {
                ctx.emit(FileUploadEvent::TerminateUploadSession(*upload_id));
                self.uploads.remove(upload_id);
                ctx.notify();
            }
        }
    }

    fn action_accessibility_contents(
        &mut self,
        _action: &Self::Action,
        _ctx: &mut ViewContext<Self>,
    ) -> warpui::accessibility::ActionAccessibilityContent {
        Default::default()
    }
}

impl View for FileUpload {
    fn ui_name() -> &'static str {
        "SSH File Upload"
    }

    fn render(&self, app: &warpui::AppContext) -> Box<dyn warpui::Element> {
        let appearance = Appearance::as_ref(app);
        self.render_file_upload_element(&self.uploads, appearance)
    }
}

/// Stats each path (blocking; call off the UI thread). A path that can't be read is not a dir.
fn probe_is_dir(paths: &[String]) -> Vec<bool> {
    paths
        .iter()
        .map(|p| Path::new(p).metadata().is_ok_and(|m| m.is_dir()))
        .collect()
}

/// Creates an sftp command that copies the local files into the PWD of the warpified ssh
/// session, if any. `is_dir[i]` says whether `local_file_paths[i]` is a directory.
fn transfer_file_sftp_command(file_upload: &FileUploadInfo, is_dir: &[bool]) -> String {
    let mut command = String::from("sftp ");
    if let Some(port) = &file_upload.remote_port {
        command += &format!("-P {port} ");
    }
    command += &file_upload.remote_host;
    command += " <<< \"";
    command += &file_upload
        .local_file_paths
        .iter()
        .zip(is_dir.iter().copied().chain(std::iter::repeat(false)))
        .map(|(path, is_dir)| sftp_put_command(path, is_dir, &file_upload.remote_dest_path))
        .join("\n");
    command += "\"";
    command
}

/// Produces a single SFTP `put` command for a given local file and remote destination.
fn sftp_put_command(
    local_file_path: &str,
    is_dir: bool,
    remote_dest_path: &Option<String>,
) -> String {
    let mut command = String::from("put ");
    if is_dir {
        command += "-r ";
    }
    command += &format!("\\\"{local_file_path}\\\"");
    if let Some(pwd) = remote_dest_path {
        command += " ";
        command += &format!("\\\"{}\\\"", &pwd);
    }
    command
}

impl FileUpload {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns whether this session has any uncleared uploads.
    /// This includes in-progress uploads, as well as completed uploads whose
    /// sessions haven't been cleared.
    pub fn has_upload(&self) -> bool {
        !self.uploads.is_empty()
    }

    pub fn generate_upload_id(&mut self) -> FileUploadId {
        let count = self.count;
        self.count += 1;
        count
    }

    /// Registers the upload and returns its id. Whether each local path is a directory (which
    /// adds `-r`) is probed off the UI thread FIRST; the sftp command is then built from that
    /// fresh answer (never a cached one) and emitted back on the main thread. If the upload was
    /// cleared while probing, nothing is emitted.
    pub fn start_file_upload(
        &mut self,
        remote_host: &str,
        local_file_paths: &[String],
        remote_dest_path: &Option<String>,
        ssh_connection: &InteractiveSshCommand,
        ctx: &mut ViewContext<Self>,
    ) -> FileUploadId {
        let upload_id = self.generate_upload_id();
        let file_upload_info = self.transfer_details(
            remote_host,
            local_file_paths,
            remote_dest_path,
            ssh_connection,
            upload_id,
        );
        self.uploads.insert(upload_id, file_upload_info);
        ctx.notify();

        let paths = local_file_paths.to_owned();
        ctx.spawn(
            async move { probe_is_dir(&paths) },
            move |me, is_dir, ctx| {
                let Some(info) = me.uploads.get(&upload_id) else {
                    return;
                };
                let command = transfer_file_sftp_command(info, &is_dir);
                ctx.emit(FileUploadEvent::CopyFileToRemote { command, upload_id });
                ctx.notify();
            },
        );
        upload_id
    }

    /// Retrieve the SSH connection information needed to start the file upload.
    fn transfer_details(
        &self,
        remote_host: &str,
        local_file_paths: &[String],
        remote_dest_path: &Option<String>,
        ssh_connection: &InteractiveSshCommand,
        upload_id: FileUploadId,
    ) -> FileUploadInfo {
        // If there's an ssh connection in a subshell, retrieve the relevant connection details.
        let remote_port = ssh_connection.port.clone();

        FileUploadInfo {
            local_file_paths: local_file_paths.to_owned(),
            remote_dest_path: remote_dest_path.clone(),
            remote_host: remote_host.to_owned(),
            remote_port,
            status: FileUploadStatus::Started,
            open_session_button: MouseStateHandle::default(),
            clear_button: MouseStateHandle::default(),
            local_session_open: false,
            upload_id,
        }
    }

    pub fn prompt_for_file_upload_password(
        &mut self,
        upload_id: FileUploadId,
        ctx: &mut ViewContext<Self>,
    ) {
        self.uploads.entry(upload_id).and_modify(|upload_info| {
            upload_info.status = FileUploadStatus::AwaitingPassword;
            ctx.notify();
        });
    }

    pub fn file_upload_finished(
        &mut self,
        upload_id: FileUploadId,
        exit_code: &ExitCode,
        ctx: &mut ViewContext<Self>,
    ) {
        self.uploads.entry(upload_id).and_modify(|upload_info| {
            let successful = exit_code.was_successful();
            upload_info.status = FileUploadStatus::Completed { successful };
            ctx.notify();
        });
    }

    pub fn local_session_state_changed(
        &mut self,
        upload_id: usize,
        local_pane_open: bool,
        ctx: &mut ViewContext<Self>,
    ) {
        self.uploads.entry(upload_id).and_modify(|upload_info| {
            if upload_info.upload_id == upload_id {
                upload_info.local_session_open = local_pane_open;
            }
            ctx.notify();
        });
    }

    fn render_single_file_upload_info(
        &self,
        file: &FileUploadInfo,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let font_family = appearance.ui_font_family();
        let font_size = appearance.ui_font_size();

        let mut button_group = Flex::row().with_main_axis_size(MainAxisSize::Min);
        button_group.add_child(self.render_view_session_button(file, appearance));

        let mut session_action_row =
            Flex::row().with_cross_axis_alignment(CrossAxisAlignment::Center);
        if let FileUploadStatus::AwaitingPassword = file.status {
            session_action_row.add_child(
                FormattedTextElement::from_str(
                    String::from("Waiting for password input"),
                    font_family,
                    font_size,
                )
                .finish(),
            );
        }

        session_action_row.add_child(button_group.finish());

        let mut file_info_and_status = Flex::column()
            .with_child(self.render_file_details(file, appearance))
            .with_child(session_action_row.finish());

        if let FileUploadStatus::Completed { successful: _ } = file.status {
            file_info_and_status = Flex::row()
                .with_child(file_info_and_status.finish())
                .with_child(self.render_clear_upload_button(file, appearance))
                .with_cross_axis_alignment(CrossAxisAlignment::Center);
        }

        let theme = appearance.theme();
        let background_color = match file.status {
            FileUploadStatus::Started => theme.accent_overlay(),
            FileUploadStatus::AwaitingPassword => theme.accent_overlay(),
            FileUploadStatus::Completed { successful: true } => theme.inactive_pane_overlay(),
            FileUploadStatus::Completed { successful: false } => {
                theme.inactive_pane_overlay().blend(&theme.accent_overlay())
            }
        }
        .into_solid();

        Container::new(file_info_and_status.finish())
            .with_background_color(background_color)
            .with_uniform_padding(4.)
            .with_uniform_margin(4.)
            .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.)))
            .finish()
    }

    fn render_file_details(
        &self,
        file: &FileUploadInfo,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        FormattedTextElement::new(
            self.render_file_detail_text(file),
            appearance.ui_font_size(),
            appearance.ui_font_family(),
            appearance.monospace_font_family(),
            appearance
                .theme()
                .main_text_color(appearance.theme().background())
                .into_solid(),
            HighlightedHyperlink::default(),
        )
        .with_heading_to_font_size_multipliers(appearance.heading_font_size_multipliers().clone())
        .finish()
    }

    /// Helper function to `render_file_details` with logic for formatted text
    /// assembly.
    fn render_file_detail_text(&self, file: &FileUploadInfo) -> FormattedText {
        let status_string = match file.status {
            FileUploadStatus::Started | FileUploadStatus::AwaitingPassword => "Uploading",
            FileUploadStatus::Completed { successful: true } => "Uploaded",
            FileUploadStatus::Completed { successful: false } => "Failed to upload",
        };

        let mut file_iter = file.local_file_paths.iter().peekable();
        let first_file = file_iter.next().expect("at least one local file path");
        let mut lines = vec![FormattedTextLine::Line(vec![
            FormattedTextFragment::plain_text(status_string),
            FormattedTextFragment::plain_text(" "),
            FormattedTextFragment::inline_code(first_file),
        ])];

        while let Some(single_file_path) = file_iter.next() {
            let line_fragments = if file_iter.peek().is_some() {
                vec![
                    FormattedTextFragment::inline_code(single_file_path),
                    FormattedTextFragment::plain_text(", "),
                ]
            } else {
                // Don't add a comma after the last file path.
                vec![FormattedTextFragment::inline_code(single_file_path)]
            };
            lines.push(FormattedTextLine::Line(line_fragments));
        }

        let mut dest_fragments = vec![
            FormattedTextFragment::plain_text(" to "),
            FormattedTextFragment::inline_code(&file.remote_host),
        ];
        if let Some(remote_path) = &file.remote_dest_path {
            dest_fragments.append(&mut vec![
                FormattedTextFragment::plain_text(":"),
                FormattedTextFragment::inline_code(remote_path),
            ]);
        }

        lines.push(FormattedTextLine::Line(dest_fragments));

        FormattedText::new(lines)
    }

    fn render_clear_upload_button(
        &self,
        file: &FileUploadInfo,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let upload_id = file.upload_id;
        let ui_builder = appearance.ui_builder().clone();
        Container::new(
            icon_button(appearance, Icon::X, true, file.clear_button.clone())
                .with_tooltip(move || {
                    ui_builder
                        .tool_tip(crate::t!("terminal-clear-upload"))
                        .build()
                        .finish()
                })
                .build()
                .on_click(move |event_ctx, _, _| {
                    event_ctx
                        .dispatch_typed_action(FileUploadAction::TerminateUploadSession(upload_id));
                })
                .finish(),
        )
        .with_margin_left(8.)
        .finish()
    }

    fn render_view_session_button(
        &self,
        file: &FileUploadInfo,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let view_session_text = if file.local_session_open {
            crate::t!("terminal-ssh-close-upload-session")
        } else {
            crate::t!("terminal-ssh-view-upload-session")
        };
        let upload_id = file.upload_id;
        Container::new(
            appearance
                .ui_builder()
                .button(ButtonVariant::Basic, file.open_session_button.clone())
                .with_text_label(view_session_text)
                .build()
                .on_click(move |event_ctx, _, _| {
                    event_ctx.dispatch_typed_action(FileUploadAction::OpenUploadSession(upload_id));
                })
                .finish(),
        )
        .with_uniform_margin(4.)
        .with_uniform_padding(4.)
        .finish()
    }

    fn render_file_upload_header(&self, appearance: &Appearance) -> Box<dyn Element> {
        Container::new(
            FormattedTextElement::new(
                FormattedText::new(vec![FormattedTextLine::Heading(FormattedTextHeader {
                    heading_size: 3,
                    text: vec![FormattedTextFragment::plain_text(crate::t!(
                        "terminal-ssh-file-uploads-title"
                    ))],
                })]),
                appearance.ui_font_size(),
                appearance.ui_font_family(),
                appearance.monospace_font_family(),
                appearance.theme().active_ui_text_color().into_solid(),
                HighlightedHyperlink::default(),
            )
            .with_heading_to_font_size_multipliers(
                appearance.heading_font_size_multipliers().clone(),
            )
            .finish(),
        )
        .with_uniform_margin(4.)
        .finish()
    }

    fn render_file_upload_element(
        &self,
        uploads: &HashMap<FileUploadId, FileUploadInfo>,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let mut upload_element = Flex::column()
            .with_cross_axis_alignment(CrossAxisAlignment::Start)
            .with_main_axis_size(MainAxisSize::Min)
            .with_child(self.render_file_upload_header(appearance));

        // Sort by ID.
        uploads
            .iter()
            .sorted_by(|upload_a, upload_b| Ord::cmp(upload_a.0, upload_b.0))
            .for_each(|(_upload_id, upload)| {
                let file_upload = self.render_single_file_upload_info(upload, appearance);
                upload_element.add_child(file_upload);
            });

        Container::new(upload_element.finish())
            .with_background_color(appearance.theme().background().into_solid())
            .with_border(
                Border::all(1.).with_border_color(appearance.theme().background().into_solid()),
            )
            .with_uniform_padding(4.)
            .with_uniform_margin(4.)
            .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.)))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(paths: &[&str]) -> FileUploadInfo {
        FileUploadInfo {
            local_file_paths: paths.iter().map(|s| s.to_string()).collect(),
            remote_dest_path: Some("/r".into()),
            remote_host: "u@h".into(),
            remote_port: Some("2222".into()),
            ..Default::default()
        }
    }

    #[test]
    fn probe_is_dir_reports_fresh_answers_per_path() {
        let dir = std::env::temp_dir().join(format!("sftp-probe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("f");
        std::fs::write(&file, b"x").unwrap();
        let paths = vec![
            dir.to_string_lossy().into_owned(),
            file.to_string_lossy().into_owned(),
            dir.join("nope").to_string_lossy().into_owned(),
        ];
        assert_eq!(probe_is_dir(&paths), vec![true, false, false]);
        // Fresh every time: the file becomes a directory-less path, the dir disappears.
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(probe_is_dir(&paths), vec![false, false, false]);
    }

    #[test]
    fn command_adds_r_only_for_directories_in_order() {
        let cmd = transfer_file_sftp_command(&info(&["/a", "/b"]), &[true, false]);
        assert_eq!(
            cmd,
            "sftp -P 2222 u@h <<< \"put -r \\\"/a\\\" \\\"/r\\\"\nput \\\"/b\\\" \\\"/r\\\"\""
        );
    }
}
