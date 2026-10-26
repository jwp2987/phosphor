# GitHub to GitLab migration (issue #989)

The project moved from GitHub (`jwp2987/phosphor`) to GitLab
(`winters-phosphor/phosphor`, project id 88085499) on 2026-10-25/26. Issue and merge-request
numbers were kept. GitHub stays up only so older releases keep downloading.

## What replaced what

| GitHub | GitLab |
|---|---|
| `.github/workflows/pr-check.yml` | `.gitlab-ci.yml`: `guards`, `check-linux`, `check-macos`, `licenses`, `test-linux`, `test-warpui-core`, `test-workspace-rest`, `integration-linux`, `integration-linux-pwsh`, `usage-test-*` |
| `.github/workflows/phosphor_release.yml` | `.gitlab-ci.yml` release pipeline: `release-meta` -> `cli-*` -> `release-*` -> `release-publish` |
| `.github/workflows/usage-test.yml` | `usage-test-linux`, `usage-test-macos` (manual) |
| `.github/ISSUE_TEMPLATE/*.yml` | `.gitlab/issue_templates/*.md` |
| `.github/pull_request_template.md` and friends | `.gitlab/merge_request_templates/` |
| `release_configurations.json` | `.gitlab/release_configurations.json` |
| `STAKEHOLDERS`, `dependabot.yml` | dropped (upstream Warp's team map and bot config) |

Not ported: Windows, macOS Intel, Linux aarch64, the rpm and Arch packages (no runner / no
`rpmbuild` or docker on the build box), and autoupdate (the GUI feature is compiled out of
shipped builds; see `script/check_no_autoupdate`). The remote-server download URL now points at
GitLab release downloads (`crates/remote_server/src/setup.rs`).

## Runners and who can start a build

Two group runners, both `run_untagged` off and `ref_protected`:

- `little-thing`: Linux x86_64 build box. Shell executor, runs as `gitlab-runner`. All cargo goes
  through `script/agent-cargo` (slot and memory governor) and `resource_group: little-thing-cargo`.
- `Mac`: macOS arm64, the maintainer's laptop. Runs as the isolated, non-admin `gitrunner` user
  (see `script/ci` history and issue #989 for the setup); its jobs are manual.

CI runs only for protected refs: `main`, `build/*` branches and `v*` tags (Maintainers only).
A branch that is not protected gets no pipeline. Job logs and artifacts are visible to project
members only (`public_jobs` off). Package-registry files are public, so uploads happen only in
tag pipelines (`script/ci/upload_package`).

## Building

1. Get `.gitlab-ci.yml` into your branch: `git merge origin/main`.
2. Push it as `build/<name>`: `git push origin <branch>:build/<name>`.
3. `guards` and `check-linux` run automatically. Builds and the heavy suites are manual jobs:
   play `build-linux-x86_64` (AppImage + deb) or `build-macos-arm64` (DMG), or the release jobs on
   a `main` dry run or a `v*` tag. A release tag must match the version in `app/Cargo.toml`.

The release pipeline pins the SHA-256 of the CLI tarballs into both clients at compile time, so
the CLI jobs run first. None of the build or release jobs had run end to end when this was written.

## Record of the `.github/` removal

Large-deletion: removed `.github/` (12 files, ~3,000 lines of GitHub Actions workflows, issue
forms and PR templates). Each one is ported to `.gitlab-ci.yml` or `.gitlab/` as listed above; git
history keeps the originals. The deleting commit (`f36bab725`) was merged without the trailer, so
its own pipeline's `guards` job failed on `script/check_large_deletions`; this document is the
written record the guard asks for.
