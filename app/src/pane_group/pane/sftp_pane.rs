//! SFTP file browser pane (a central pane, opened via the SSH manager tree).
//!
//! Modeled on the minimal structure of `ssh_server_pane.rs`. The pane is not
//! persisted (`LeafContents::Sftp { .. }` returns false from
//! `is_persisted()`); business data is driven by SFTP connection operations.
//! author: logic
//! date: 2026-05-26

use warpui::{AppContext, ModelHandle, View, ViewContext, ViewHandle};

use crate::pane_group::{
    BackingView, PaneConfiguration, PaneContent, PaneGroup, PaneView,
};
use crate::app_state::LeafContents;
use crate::sftp_manager::browser::SftpBrowserView;

use super::{DetachType, PaneId, ShareableLink, ShareableLinkError};

/// SFTP file browser pane content
pub struct SftpPane {
    view: ViewHandle<PaneView<SftpBrowserView>>,
    pane_configuration: ModelHandle<PaneConfiguration>,
    /// The business node id (not the pane view id), used for snapshot serialization.
    node_id: String,
}

impl SftpPane {
    /// Creates a new SFTP browser pane
    pub fn new<V: View>(node_id: String, ctx: &mut ViewContext<V>) -> Self {
        let id_for_view = node_id.clone();
        let browser_view =
            ctx.add_typed_action_view(move |ctx| SftpBrowserView::new(id_for_view.clone(), ctx));
        let pane_configuration = browser_view.as_ref(ctx).pane_configuration();
        let pane_view = ctx.add_typed_action_view(|ctx| {
            let pane_id = PaneId::from_sftp_pane_ctx(ctx);
            PaneView::new(pane_id, browser_view, (), pane_configuration.clone(), ctx)
        });
        Self {
            view: pane_view,
            pane_configuration,
            node_id,
        }
    }
}

impl PaneContent for SftpPane {
    fn id(&self) -> PaneId {
        PaneId::from_sftp_pane_view(&self.view)
    }

    fn attach(
        &self,
        _group: &PaneGroup,
        focus_handle: crate::pane_group::focus_state::PaneFocusHandle,
        ctx: &mut ViewContext<PaneGroup>,
    ) {
        self.view
            .update(ctx, |view, ctx| view.set_focus_handle(focus_handle, ctx));
        let child = self.view.as_ref(ctx).child(ctx);

        let pane_id = self.id();
        ctx.subscribe_to_view(&child, move |pane_group, _, event, ctx| {
            pane_group.handle_pane_event(pane_id, event, ctx);
        });
    }

    fn detach(
        &self,
        _group: &PaneGroup,
        _detach_type: DetachType,
        ctx: &mut ViewContext<PaneGroup>,
    ) {
        let child = self.view.as_ref(ctx).child(ctx);
        ctx.unsubscribe_to_view(&child);
    }

    fn snapshot(&self, _ctx: &AppContext) -> LeafContents {
        LeafContents::Sftp {
            node_id: self.node_id.clone(),
        }
    }

    fn has_application_focus(&self, ctx: &mut ViewContext<PaneGroup>) -> bool {
        self.view.is_self_or_child_focused(ctx)
    }

    fn focus(&self, ctx: &mut ViewContext<PaneGroup>) {
        self.view
            .as_ref(ctx)
            .child(ctx)
            .update(ctx, BackingView::focus_contents)
    }

    fn shareable_link(
        &self,
        _ctx: &mut ViewContext<PaneGroup>,
    ) -> Result<ShareableLink, ShareableLinkError> {
        Ok(ShareableLink::Base)
    }

    fn pane_configuration(&self) -> ModelHandle<PaneConfiguration> {
        self.pane_configuration.clone()
    }

    fn is_pane_being_dragged(&self, ctx: &AppContext) -> bool {
        self.view.as_ref(ctx).is_being_dragged()
    }
}
