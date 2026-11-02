# Release signing and notarization: current state and what it needs

Status (#1515): **nothing we publish is signed or notarized by a trusted identity**, and no signing credentials exist in
CI. This page says what is checked today and exactly what the owner must supply to change that. Nothing here invents or
assumes a secret.

## What is verified today (`script/ci/verify_artifacts.sh` / `.ps1`)

Run by every `build-*` and `release-*` job on its own output, in the release jobs BEFORE `upload_package` publishes anything.

| platform | fails the job | reported only |
|---|---|---|
| all | `SHA256SUMS` / `manifest-*.txt` missing or empty; a listed file absent or with a different digest; an artifact not listed | |
| Linux | a `.deb` that `dpkg-deb --info/--contents` cannot read, has an empty payload, or whose `dpkg -x` scratch-root unpack lacks an ELF app binary or a `.desktop` entry | the `.deb`/AppImage carry no signature of any kind |
| macOS | `hdiutil verify` of the DMG fails | `codesign --verify --deep --strict` and `spctl --assess` of the `.app` inside the DMG, printed with an `UNSIGNED` note. The app is ad-hoc (or "Apple Development") signed by `--selfsign`, which gives it a cdhash but is not Developer ID and is not notarized, so `spctl` rejects it |
| Windows | | `Get-AuthenticodeSignature` status per `.exe`, `UNSIGNED` note while `NotSigned` |

Set `PHOSPHOR_REQUIRE_SIGNED=1` (job variable) to turn the "reported only" signature results into failures. Do that only
after signing is in place.

Not covered (still manual, see `docs/testing.md` section 5): Gatekeeper first-launch behaviour (`app_test.sh` strips
quarantine), SmartScreen, `dpkg -i` on a real system, and install/upgrade/uninstall cases.

## What the owner must provide to enable signing

All as GitLab CI/CD variables on `winters-phosphor/phosphor`, **Protected** (so only `main`, `build/*`, `v*` pipelines
see them), **Masked** where the value allows it, and **Hidden** for files. None of these may go in the repo, `ci.env`
or a job log.

### macOS (Developer ID signing + notarization)

Requires a paid Apple Developer Program membership.

1. A **Developer ID Application** certificate and its private key, exported as a `.p12`, base64-encoded:
   `APPLE_DEVELOPER_ID_CERT_P12_B64` (file or masked variable) and `APPLE_DEVELOPER_ID_CERT_PASSWORD` (masked).
2. A throwaway keychain password for the job's temporary keychain: `CODESIGN_KEYCHAIN_PASSWORD` (masked; any random value).
3. Notarization credentials for `xcrun notarytool`: the **Apple ID** (`NOTARIZATION_APPLE_ID`), an **app-specific
   password** created at appleid.apple.com (`NOTARIZATION_PASSWORD`, masked), and the **Team ID** (`APPLE_TEAM_ID`).
   Better than passing them each time: store a notarytool profile once on the Mac runner
   (`xcrun notarytool store-credentials phosphor-notary --apple-id ... --team-id ... --password ...`) and have the job
   use `--keychain-profile phosphor-notary`; then only the profile name is a variable and the secret stays in the runner
   user's keychain.
4. Code changes needed: `script/macos/bundle`'s `--codesign` path still reads `WARP_*` variable names and hard-codes
   Warp's team id (`APPLE_TEAM_ID="2BBY89MBSN"`, line ~733); it must be switched to the variables above, the
   `release-macos-arm64` job must call it with `--codesign` instead of `--selfsign`, and `script/ci/verify_artifacts.sh`
   should then run with `PHOSPHOR_REQUIRE_SIGNED=1` plus `xcrun stapler validate`.
5. The Mac runner needs a GUI (Aqua) session for create-dmg's Finder step, or `PHOSPHOR_DMG_SKIP_JENKINS=1` as today.

### Windows (Authenticode)

1. A code-signing certificate (OV, or EV to avoid SmartScreen reputation warm-up) as a `.pfx`, base64-encoded:
   `WINDOWS_CODESIGN_PFX_B64` and `WINDOWS_CODESIGN_PFX_PASSWORD` (masked). An EV or cloud HSM certificate cannot be
   exported; then the variable is the vendor's signing-service credentials instead.
2. `SIGN_TOOL_CMD`: the `signtool sign ...` command line `script/windows/bundle.ps1` already accepts (it passes it to
   Inno Setup as `/Scodesign=`). The job would materialize the `.pfx` to a temp file, set `SIGN_TOOL_CMD`, and delete it in
   `after_script`. A timestamp server URL (RFC 3161) must be part of the command, or signatures expire with the cert.

### Linux

Optional: a GPG key for detached signatures (`SHA256SUMS.asc`): `RELEASE_GPG_PRIVATE_KEY` (masked, base64) and
`RELEASE_GPG_PASSPHRASE`. Publish the public key in the repo and docs. There is no equivalent of Gatekeeper to satisfy.

### Rules for whoever wires it

* Only on protected refs; never echo a variable; no `set -x` in these steps.
* The scratch keychain/pfx is created in `$CI_PROJECT_DIR`-external temp space and removed in `after_script`, which must
  run on cancel too (see the P1-15 note in `docs/testing.md` on teardown).
* Dry runs on `main` must not sign with release credentials unless the owner says so.
