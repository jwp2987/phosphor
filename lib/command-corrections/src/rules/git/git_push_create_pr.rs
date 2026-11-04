use lazy_static::lazy_static;
use regex::Regex;

use crate::{
    default_rule_id,
    rules::{util::open_in_default_application, Rule},
    Command, RuleCorrection, SessionMetadata,
};

/// Suggests creating a new pull request on GitHub or GitLab when pushing a new branch.
pub struct GitPushCreatePullRequest;

const GITHUB_MARKER: &str = "create a pull request for .* on github by visiting:";
/// The message that GitLab sends when [pushing a new branch](https://docs.gitlab.com/ee/user/project/merge_requests/creating_merge_requests.html#when-you-use-git-commands-locally).
const GITLAB_MARKER: &str = "to create a merge request for .*, visit:";

lazy_static! {
    /// Match the "create a pull request" link that GitHub and GitLab print
    /// when pushing a new branch.
    /// Both display a message followed by the URL to open on a separate line.
    /// Git also prefixes each line of output from the remote server with
    /// `remote:`, so we have to skip past that.
    static ref REGEX: Regex = Regex::new(&format!(
        r"(?mi)(?:{GITHUB_MARKER}|{GITLAB_MARKER})\s+(?:remote:\s*)?(\S+)$"
    ))
    .unwrap();
}

impl Rule for GitPushCreatePullRequest {
    default_rule_id!(GitPushCreatePullRequest);

    fn should_be_considered_by_default(
        &self,
        command: &Command,
        _session_metadata: &SessionMetadata,
    ) -> bool {
        // Unlike corrections for failed commands, this rule only makes sense after a successful push.
        command.exit_code.is_success()
    }

    fn matches(&self, command: &Command, _session_metadata: &SessionMetadata) -> bool {
        command.input_parts().iter().any(|part| part == "push") && REGEX.is_match(command.output)
    }

    fn generate_command_corrections<'a>(
        &self,
        command: &'a Command,
        session_metadata: &'a SessionMetadata,
    ) -> Option<Vec<RuleCorrection<'a>>> {
        REGEX
            .captures(command.output)
            .and_then(|captures| captures.get(1))
            .and_then(|url| open_in_default_application(session_metadata, url.as_str()))
            .map(|correction| vec![correction])
    }
}

#[cfg(test)]
mod tests {
    use crate::{test_utils::corrections_given_metadata, Command, ExitCode, SessionMetadata};

    /// Produce corrections for a successful push command.
    /// This is based on [`crate::test_utils::basic_corrections`].
    fn success_corrections(input: &str, output: &str) -> Vec<String> {
        let mut metadata = SessionMetadata::new();
        metadata.set_executables(["open"]);
        let command = Command::new(input, output, ExitCode(0));
        corrections_given_metadata(command, &metadata)
    }

    #[test]
    fn test_git_push_with_github() {
        assert_eq!(
            success_corrections(
                "git push --set-upstream origin my-change",
                "
                Enumerating objects: 7, done.
                Counting objects: 100% (7/7), done.
                Delta compression using up to 12 threads
                Compressing objects: 100% (3/3), done.
                Writing objects: 100% (4/4), 402 bytes | 402.00 KiB/s, done.
                Total 4 (delta 2), reused 0 (delta 0), pack-reused 0
                remote: Resolving deltas: 100% (2/2), completed with 2 local objects.
                remote:
                remote: Create a pull request for 'my-change' on GitHub by visiting:
                remote:      https://github.com/warpdotdev/some-repo/pull/new/my-change
                remote:
                To github.com:warpdotdev/some-repo.git
                * [new branch]      my-change -> my-change
                branch 'my-change' set up to track 'origin/my-change'.
                "
            ),
            vec!["open https://github.com/warpdotdev/some-repo/pull/new/my-change"]
        )
    }

    #[test]
    fn test_git_push_with_gitlab() {
        assert_eq!(
            success_corrections("git push origin my-new-branch",
                "
                Enumerating objects: 7, done.
                Counting objects: 100% (7/7), done.
                Delta compression using up to 12 threads
                Compressing objects: 100% (3/3), done.
                Writing objects: 100% (4/4), 402 bytes | 402.00 KiB/s, done.
                Total 4 (delta 2), reused 0 (delta 0), pack-reused 0
                remote: Resolving deltas: 100% (2/2), completed with 2 local objects.
                remote: To create a merge request for my-new-branch, visit:
                remote:   https://gitlab.warp.dev/my-group/my-project/merge_requests/new?merge_request%5Bsource_branch%5D=my-new-branch
                remote:
                To gitlab.warp.dev:my-group/my-project.git
                * [new branch]      my-new-branch -> my-new-branch
                "), 
            vec!["open 'https://gitlab.warp.dev/my-group/my-project/merge_requests/new?merge_request%5Bsource_branch%5D=my-new-branch'"])
    }

    #[test]
    fn test_git_push_cannot_open() {
        let metadata = SessionMetadata::new();
        let command = Command::new(
            "git push --set-upstream origin my-change",
            "
            Enumerating objects: 7, done.
            Counting objects: 100% (7/7), done.
            Delta compression using up to 12 threads
            Compressing objects: 100% (3/3), done.
            Writing objects: 100% (4/4), 402 bytes | 402.00 KiB/s, done.
            Total 4 (delta 2), reused 0 (delta 0), pack-reused 0
            remote: Resolving deltas: 100% (2/2), completed with 2 local objects.
            remote:
            remote: Create a pull request for 'my-change' on GitHub by visiting:
            remote:      https://github.com/warpdotdev/some-repo/pull/new/my-change
            remote:
            To github.com:warpdotdev/some-repo.git
            * [new branch]      my-change -> my-change
            branch 'my-change' set up to track 'origin/my-change'.
            ",
            ExitCode(0),
        );
        assert_eq!(
            corrections_given_metadata(command, &metadata),
            Vec::<String>::new()
        );
    }

    #[test]
    fn test_git_push_on_linux() {
        let mut metadata = SessionMetadata::new();
        metadata.set_executables(["xdg-open"]);
        let command = Command::new(
            "git push --set-upstream origin my-change",
            "
            Enumerating objects: 7, done.
            Counting objects: 100% (7/7), done.
            Delta compression using up to 12 threads
            Compressing objects: 100% (3/3), done.
            Writing objects: 100% (4/4), 402 bytes | 402.00 KiB/s, done.
            Total 4 (delta 2), reused 0 (delta 0), pack-reused 0
            remote: Resolving deltas: 100% (2/2), completed with 2 local objects.
            remote:
            remote: Create a pull request for 'my-change' on GitHub by visiting:
            remote:      https://github.com/warpdotdev/some-repo/pull/new/my-change
            remote:
            To github.com:warpdotdev/some-repo.git
            * [new branch]      my-change -> my-change
            branch 'my-change' set up to track 'origin/my-change'.
            ",
            ExitCode(0),
        );
        assert_eq!(
            corrections_given_metadata(command, &metadata),
            vec!["xdg-open https://github.com/warpdotdev/some-repo/pull/new/my-change"]
        );
    }

    #[test]
    fn test_git_push_without_remote() {
        assert_eq!(
            success_corrections(
                "git push --set-upstream origin my-change",
                "
                Resolving deltas: 100% (2/2), completed with 2 local objects.

                Create a pull request for 'my-change' on GitHub by visiting:
                      https://github.com/warpdotdev/some-repo/pull/new/my-change

                branch 'my-change' set up to track 'origin/my-change'.
                "
            ),
            vec!["open https://github.com/warpdotdev/some-repo/pull/new/my-change"]
        )
    }

    #[test]
    fn test_git_push_no_suggestion() {
        assert_eq!(
            success_corrections(
                "git push --set-upstream origin my-change",
                "
                Enumerating objects: 7, done.
                Counting objects: 100% (7/7), done.
                Delta compression using up to 12 threads
                Compressing objects: 100% (3/3), done.
                Writing objects: 100% (4/4), 402 bytes | 402.00 KiB/s, done.
                Total 4 (delta 2), reused 0 (delta 0), pack-reused 0
                remote: Resolving deltas: 100% (2/2), completed with 2 local objects.
                To github.com:warpdotdev/some-repo.git
                * [new branch]      my-change -> my-change
                branch 'my-change' set up to track 'origin/my-change'.
                "
            ),
            Vec::<String>::new()
        )
    }

    #[test]
    fn test_git_push_missing_url() {
        assert_eq!(
            success_corrections(
                "git push --set-upstream origin my-change",
                "
                Enumerating objects: 7, done.
                Counting objects: 100% (7/7), done.
                Delta compression using up to 12 threads
                Compressing objects: 100% (3/3), done.
                Writing objects: 100% (4/4), 402 bytes | 402.00 KiB/s, done.
                Total 4 (delta 2), reused 0 (delta 0), pack-reused 0
                remote: Resolving deltas: 100% (2/2), completed with 2 local objects.
                remote:
                remote: Create a pull request for 'my-change' on GitHub by visiting:
                "
            ),
            Vec::<String>::new()
        )
    }

    #[test]
    fn test_failed_push() {
        let mut metadata = SessionMetadata::new();
        metadata.set_executables(["open"]);
        let command = Command::new(
            "git push orgn my-branch",
            "
            fatal: 'orgn' does not appear to be a git repository
            fatal: Could not read from remote repository.

            Please make sure you have the correct access rights
            and the repository exists.
            ",
            ExitCode(1),
        );
        assert_eq!(
            corrections_given_metadata(command, &metadata),
            Vec::<String>::new()
        );
    }

    #[test]
    fn test_unrelated_git_command() {
        assert_eq!(
            success_corrections(
                "git log --oneline",
                "
                abc123 This is a feature commit
                d1a78b Create a pull request for all the bug fixes
                90gg64 This commit added a bug
                88762c So did this one
                "
            ),
            Vec::<String>::new()
        );
    }
}
