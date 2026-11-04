use std::str::FromStr;

use serde::{Deserialize, Serialize};

use warpui::{AppContext, SingletonEntity};

use crate::{
    auth::UserUid, cloud_object::model::persistence::ObjectStoreModel, server::ids::ServerId,
};

// Zap Phase 2a: `dialog/` (cloud sharing modal UI) deleted along with
// all consumer triggers. `style.rs` is retained because the Subject /
// UserKind avatar helpers in this module still depend on it.
mod style;

#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SharingAccessLevel {
    View,
    Edit,
    Full,
}

impl SharingAccessLevel {
    pub fn label(&self) -> &'static str {
        match self {
            SharingAccessLevel::View => "Can view",
            SharingAccessLevel::Edit => "Can edit",
            SharingAccessLevel::Full => "Full access",
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            SharingAccessLevel::View => "view",
            SharingAccessLevel::Edit => "edit",
            SharingAccessLevel::Full => "access",
        }
    }

    pub fn can_trash(self) -> bool {
        self >= SharingAccessLevel::Edit
    }

    pub fn can_delete(self) -> bool {
        self >= SharingAccessLevel::Full
    }

    pub fn can_move_drive(self) -> bool {
        self >= SharingAccessLevel::Full
    }

    pub fn to_serializable_value(self) -> &'static str {
        match self {
            SharingAccessLevel::View => "VIEW",
            SharingAccessLevel::Edit => "EDIT",
            SharingAccessLevel::Full => "FULL",
        }
    }
}

impl FromStr for SharingAccessLevel {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "VIEW" => Ok(Self::View),
            "EDIT" => Ok(Self::Edit),
            "FULL" => Ok(Self::Full),
            _ => Err(anyhow::anyhow!("unknown access level {value}")),
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum LinkSharingSubjectType {
    None,
    Anyone,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Subject {
    User(UserKind),
    #[allow(dead_code)]
    PendingUser {
        email: Option<String>,
    },
    Team(TeamKind),
    AnyoneWithLink(LinkSharingSubjectType),
}

#[derive(Debug, Clone)]
pub enum UserKind {
    Account(UserUid),
}

#[derive(Debug, Clone, PartialEq)]
pub enum TeamKind {
    Team { team_uid: ServerId },
}

impl TeamKind {
    pub fn team_uid(&self) -> ServerId {
        match self {
            TeamKind::Team { team_uid } => *team_uid,
        }
    }
}

impl Subject {
    pub fn user_uid(&self) -> Option<UserUid> {
        match self {
            Subject::User(user_kind) => match user_kind {
                UserKind::Account(user_uid) => Some(*user_uid),
            },
            Subject::PendingUser { .. } | Subject::Team(_) | Subject::AnyoneWithLink(_) => None,
        }
    }

    pub fn is_user(&self, other_uid: UserUid) -> bool {
        match self {
            Subject::User(UserKind::Account(user_uid)) => *user_uid == other_uid,
            Subject::PendingUser { .. } | Subject::Team(_) | Subject::AnyoneWithLink(_) => false,
        }
    }

    pub fn team_uid(&self) -> Option<ServerId> {
        match self {
            Subject::Team(team_kind) => Some(team_kind.team_uid()),
            Subject::User(_) | Subject::PendingUser { .. } | Subject::AnyoneWithLink(_) => None,
        }
    }
}

impl PartialEq for UserKind {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Account(self_uid), Self::Account(other_uid)) => self_uid == other_uid,
        }
    }
}

/// Identifier for an object that's shareable via the Zap Drive ACL model. Not all sharing in Zap
/// is _currently_ tied into this model (e.g. block sharing).
#[derive(Debug, Clone)]
#[allow(dead_code)] // persisted or derived enum
pub enum ShareableObject {
    /// A shareable Zap Drive object.
    WarpDriveObject(ServerId),
}

impl ShareableObject {
    /// The canonical link to this object.
    #[allow(dead_code)] // used by sibling unused code or shared name
    pub fn link(&self, app: &AppContext) -> Option<String> {
        match self {
            ShareableObject::WarpDriveObject(id) => ObjectStoreModel::as_ref(app)
                .get_by_uid(&id.uid())
                .and_then(|object| object.object_link()),
        }
    }
}

/// Whether not a shared object's contents are editable by the current user.
///
/// This is not purely a function of their access level since anonymous users are not allowed to
/// edit (due to the lack of attribution).
#[derive(Debug, Clone, Copy)]
pub enum ContentEditability {
    ReadOnly,
    RequiresLogin,
    Editable,
}

impl ContentEditability {
    pub fn can_edit(self) -> bool {
        matches!(self, ContentEditability::Editable)
    }
}
