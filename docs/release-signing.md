# Release signing and notarization: current state and what it needs

Status (#1515): **nothing we publish is signed or notarized by a trusted identity** yet: the signing steps exist in CI but
no credentials do. This page says what is checked today and exactly what the owner must supply to change that. Nothing here invents or
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

## How signing is wired (done; waiting on credentials)

`release-macos-arm64` and `release-windows-x86_64` in `.gitlab-ci.yml` sign **only when every variable below exists**.
With any of them missing they do exactly what they did before (macOS: ad-hoc `--selfsign`; Windows: unsigned) and
`verify_artifacts` keeps only reporting. When signing ran, the job sets `PHOSPHOR_REQUIRE_SIGNED=1` so an invalid
signature fails the release before `upload_package`, and macOS additionally runs `xcrun stapler validate` on the DMG.
`script/macos/bundle` itself has no Warp team id, identity or `WARP_*` variable any more (they were Warp's
`2BBY89MBSN`, the zach@warp.dev notarization account and `WARP_NOTARIZATION_PASSWORD` / `WARP_DEVELOPER_ID_CERT_PASSWORD`
/ `WARP_CODESIGN_KEYCHAIN_PASSWORD`); with no identity it signs ad-hoc (`codesign -s -`), `--nosign` skips signing.
`script/Entitlements.plist` no longer claims Warp's App Group (`2BBY89MBSN.dev.warp`); that group belongs to Warp's
team and cannot be claimed by a Phosphor-signed app.

**Untested.** This was written without a Mac, a certificate or a pipeline run. The first signed run must be a
deliberate, watched one (manual job on `main`, or a throwaway `v*` tag) before any real release depends on it.

## What the owner must create and provide

All as GitLab CI/CD variables on `winters-phosphor/phosphor`, **Protected** (so only `main`, `build/*`, `v*` pipelines
see them), **Masked** where the value allows it. None of these may go in the repo, `ci.env` or a job log. Names are
exactly as the scripts read them.

### macOS (Developer ID signing + notarization)

Requires a paid Apple Developer Program membership.

1. In the Apple Developer portal create a **Developer ID Application** certificate (not "Apple Development"). Export it
   with its private key from Keychain Access as a `.p12`, then `base64 -i cert.p12 | tr -d '\n'`.
   * `PHOSPHOR_DEVELOPER_ID_CERT`: that base64 text (masked).
   * `PHOSPHOR_DEVELOPER_ID_CERT_PASSWORD`: the password chosen at export (masked).
   * `PHOSPHOR_APPLE_SIGNING_IDENTITY`: the certificate's common name, e.g. `Developer ID Application: Your Name (ABCDE12345)`.
   * `PHOSPHOR_APPLE_TEAM_ID`: the 10-character Team ID (Membership page).
2. `PHOSPHOR_CODESIGN_KEYCHAIN_PASSWORD`: any random string (masked); it only protects the throwaway keychain the job
   creates and deletes.
3. Notarization, either:
   * `PHOSPHOR_NOTARIZATION_APPLE_ID` (the Apple ID email) and `PHOSPHOR_NOTARIZATION_PASSWORD`, an **app-specific
     password** made at appleid.apple.com (masked); or
   * preferred: once, on the Mac runner as the runner user, run
     `xcrun notarytool store-credentials phosphor-notary --apple-id ... --team-id ... --password ...` (or an App Store
     Connect API key via `--key/--key-id/--issuer`), then set only `PHOSPHOR_NOTARY_PROFILE=phosphor-notary`. The secret
     then stays in that user's keychain and never enters GitLab.
4. The Mac runner needs a GUI (Aqua) session for create-dmg's Finder step, or `PHOSPHOR_DMG_SKIP_JENKINS=1` as today.
5. A manual `release-macos-arm64` run on `main` also signs if the variables are visible to `main` (they are, when
   protected). Delete or unprotect the variables if you do not want dry runs to use the certificate.

### Windows (Authenticode)

1. A code-signing certificate (OV, or EV to avoid SmartScreen reputation warm-up) as a `.pfx`:
   * `WINDOWS_CODESIGN_PFX_B64`: `base64` of the `.pfx` (masked).
   * `WINDOWS_CODESIGN_PFX_PASSWORD`: its password (masked).
   * `WINDOWS_CODESIGN_TIMESTAMP_URL`: your CA's RFC 3161 timestamp server URL. Required, and deliberately has no
     default here: without a timestamp the signature dies with the certificate.
2. The release job writes the `.pfx` to `%TEMP%`, builds `SIGN_TOOL_CMD` (`signtool sign /fd SHA256 /td SHA256 /tr <url>
   /f <pfx> /p <password> $f`, which `script/windows/bundle.ps1` hands to Inno Setup) and deletes the `.pfx` in
   `after_script`. `signtool.exe` (Windows SDK) must be on the Windows runner's `PATH`.
3. EV certificates and cloud HSMs (Azure Trusted Signing, DigiCert KeyLocker) cannot be exported as a `.pfx`; if that is
   your choice, say so and `SIGN_TOOL_CMD` will be set from the vendor's tool instead.

### Linux

Optional: a GPG key for detached signatures (`SHA256SUMS.asc`): `RELEASE_GPG_PRIVATE_KEY` (masked, base64) and
`RELEASE_GPG_PASSPHRASE`. Publish the public key in the repo and docs. There is no equivalent of Gatekeeper to satisfy.

### Rules for whoever wires it

* Only on protected refs; never echo a variable; no `set -x` in these steps.
* The scratch keychain/pfx is created in `$CI_PROJECT_DIR`-external temp space and removed in `after_script`, which must
  run on cancel too (see the P1-15 note in `docs/testing.md` on teardown).
* Dry runs on `main` must not sign with release credentials unless the owner says so.
