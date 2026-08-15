mod batch;
mod comment;
pub(crate) mod convert;
mod diff_hunk_parser;
mod flatten;
mod pending_imported;

pub(crate) use batch::{ReviewCommentBatch, ReviewCommentBatchEvent};
pub(crate) use comment::{
    AttachedReviewComment, AttachedReviewCommentTarget, CommentId, CommentOrigin, LineDiffContent,
};
// Re-exported for API completeness rather than for a use inside this module:
// `PendingImportedReviewComment::github_details` is `pub(crate)` and typed with
// it, and `comments::comment` is private, so this is the only path by which the
// rest of the crate can name the type. Today only `code_review_view_tests` does,
// so a lib-only build (no test target) sees the re-export as unused.
#[allow(unused_imports)]
pub(crate) use comment::ImportedCommentDetails;
pub(crate) use convert::convert_insert_review_comments;
pub(crate) use flatten::attach_pending_imported_comments;
pub(crate) use pending_imported::{
    PendingImportedReviewComment, PendingImportedReviewCommentTarget,
};
