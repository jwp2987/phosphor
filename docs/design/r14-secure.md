# r14-secure: the Linux fallback key (#1932) and query predictions in ssh sessions (#1982)

Owner decisions 2026-11-10. Nothing here was compiled or run when it was written (laptop rule);
the first box run decides. Both are app changes; neither is a harness fault.

## 1. #1932: the Linux secure-storage fallback key

### Cause

`crates/warpui_extras/src/secure_storage/linux.rs` (before this change, `encryption_key`,
lines 136-162): when the Secret Service cannot take a value, it is written to
`<state_dir>/<service>-<key>` as `nonce || AES-256-GCM(value)`, mode 0600, and the AES key is a
string literal compiled into the binary. Upstream does the same with another literal
(`42effe840:crates/warpui_extras/src/secure_storage/linux.rs:109`). Anyone who holds a blob and
the public binary can open it; the file mode is the only protection.

The earlier note (`docs/design/secure-storage-fallback-key.md`, 2026-11-09) held the code back
for three owner decisions. The owner has now decided: do it, the per-install secret lives in the
keyring, existing files migrate, and there is no silent fallback to a built-in key.

### What the keyring can and cannot do here

The fallback file exists for the moment the keyring cannot be used, and that is also the moment
the keyring cannot hand out a secret. So "the secret is in the keyring" covers one case only: the
keyring answers, but would not store this value (a refused item, a failed write). On a machine
with no Secret Service at all (the common fallback user: a headless box, a minimal window
manager, an ssh login) there is no keyring to hold anything. The owner's instruction names that
case and asks for a decision between a key file and refusing.

### Chosen design

**One per-install secret, two possible homes, recorded in the blob.**

- Blob format: `PSF1 || source tag || nonce(12) || ciphertext+tag`. The tag is `K` (key derived
  from the secret in the keyring) or `F` (key derived from the key file). The 5-byte header is
  the AEAD associated data. A blob without the header is a legacy blob.
- Key derivation: HKDF-SHA256 (`ring::hkdf`, already a dependency), salt
  `phosphor secure-storage fallback v1`, info = source label, NUL, service name; output is the
  AES-256-GCM key. The two sources and two service names never share a key.
- **Keyring home (`K`).** The secret is 32 random bytes, hex, in the keyring item
  `SecureStorageFallbackSecret` under the app's service name. It is read (and created if absent)
  only when a value has to go to the fallback while a keyring call has succeeded this run
  (`availability::current() == Available`). No keyring round trip is added to startup or to any
  path of a user whose keyring works; the keyring gains the item only on the first such
  fallback write. A stored secret that does not decode is never overwritten (blobs sealed with
  the real one may exist); the write then uses the key file.
- **No keyring (`F`): a random key file, not a refusal.** `<state_dir>/<service>-secure-storage.key`,
  32 random bytes, mode 0600, beside the blobs. Decision and reason: refusing would take away
  stored API keys, MCP tokens and the proxy password from every keyring-less Linux user on
  upgrade, which is the population the fallback (and the `keyring_unavailable_startup` case)
  exists for; the key file is strictly better than today for them. Its limit is stated, not
  hidden: it stops a blob copied without the key file (a partial backup, a pasted file, offline
  use of the public binary), it does not stop a process running as the user, which can read both
  files. Only a keyring does. The warning logged at each fallback write says which key was used
  and what it does not defend against, and the manual says the same.
- **Never the built-in key for a write.** It is kept only to open legacy blobs. If the key file
  cannot be created (missing or read-only directory) or exists but is unusable (wrong length,
  unreadable), the write fails with an error naming the file. A key file is never regenerated:
  a new secret would orphan every blob sealed with the old one and hide the loss.
- **Key file creation is atomic across processes** (GUI, `phosphor-tui`, two app starts): the
  secret is written to a temporary name and hard-linked to the final name, which fails if the
  name is taken; the loser reads the winner's file. A filesystem without hard links gets an
  exclusive create of the name itself.
- **Migration, lazy, on read.** `read_value` reads a legacy blob with the built-in key as
  before. If the keyring is reachable the value moves into the keyring and the blob is deleted
  (unchanged #1805 behaviour, so nothing is re-sealed and no key file appears). Otherwise the
  value is re-sealed with the current key, the new blob is opened again and compared, and only
  then renamed over the legacy blob (temporary file, fsync, rename). Any failure leaves the
  legacy blob byte for byte, still readable, and logs a warning. There is no bulk pass at
  startup: every credential in use is read at startup anyway, and one that is never read is
  never touched.
- **A blob that exists but cannot be opened is an error at the fallback layer, never "not
  found"** (lost key file, `K` blob while the keyring is away, damaged blob). `read_value` then
  logs it and consults the keyring, as it did for unreadable blobs before; the blob stays in
  place, so it reads again when the key file or the keyring is back.

### Rejected

- **Refuse when there is no keyring.** See above: loses working credentials for the users the
  fallback serves.
- **Always the key file, never the keyring.** Simpler, and what the earlier note proposed, but
  the owner decided the keyring holds the secret where one exists.
- **Read the keyring secret at startup and cache it**, so that a keyring that goes away mid-run
  still yields `K` blobs. It adds a D-Bus round trip and a keyring item for every user, to
  produce blobs that cannot be read on the next start without a keyring.
- **Kernel keyring (`keyctl`).** Still a separate decision; not needed for this change.
- **Atomic writes for ordinary fallback writes.** Not part of this change (`DECLINED.md` notes
  the blobs' own 0600 path); only the migration replaces a file that holds the only copy of a
  value, so only it uses rename.

### Risk

- **Downgrade.** A build before this change cannot read `PSF1` blobs: after an upgrade and one
  start, a downgrade to v0.1.17 or earlier shows keyring-less users no stored credentials (the
  files stay on disk and read again after upgrading). Owner question 2 of the earlier note; the
  decision to migrate implies it, and it is listed in the report.
- **`K` blobs need the keyring.** A value written while the keyring answered-but-refused cannot
  be read in a later run with no keyring. Before, it could. It reads as "not saved" (with a
  logged error); saving again writes an `F` blob over it.
- **Two processes creating the keyring secret at once**: the Secret Service has no
  create-if-absent. Each process reads the secret back after writing it, which leaves a window
  of one D-Bus call in which a process could seal one blob with a secret the other then
  replaces. It needs two first-ever fallback writes at the same moment with a keyring that
  refuses both values.
- **Losing the key file loses the `F` blobs.** A state directory restored without it, or
  cleaned by hand. The error is logged with the file name; the manual says to keep the two
  together.
- **Not compiled.** `ring::hkdf` types (`Okm` into `UnboundKey`), `split_at_checked`.

### Tests (`secure_storage/linux_test.rs`)

Key derivation (same inputs same key; secret, source, service and associated data each change
it); hex round trip of the keyring secret; no keyring: `F` blob, 0600 key file, built-in key
does not open it, a second handle reads it; legacy blob re-sealed on read, value identical, no
temporary file left; failed re-seal keeps the legacy blob; legacy blob with a working keyring
moves to the keyring with no key file; lost and wrong-length key file: error that is not
`NotFound`, never replaced, write refused, reads again when restored; tampered header and body;
eight threads creating the key file settle on one secret; keyring that refuses one value: `K`
blob, secret in the keyring once, no key file, unreadable without the keyring and left in place;
malformed keyring secret not replaced. The existing tests of the built-in key now call
`legacy_encrypt` / `legacy_decrypt`: same assertions, the functions were renamed because that key
no longer seals anything.

`keyring_unavailable_startup` (#1805) is unchanged and stays consistent: this change adds no
keyring call at startup, no `Failed to acquire default Secret Service collection` line and no
ERROR line on a healthy run. In its `absent` and `hang` launches a credential write now also
creates the key file; nothing in the case reads the state directory listing.

## 2. #1982: no query prediction in ssh sessions by default

### Cause

`app/src/terminal/input.rs`: every edit of a locked AI input queues `predict_am_query` behind a
250 ms debounce (`DEBOUNCE_AI_QUERY_PREDICTION_PERIOD`, line 426; trigger at 10533-10546), which
builds a request from the input buffer through
`active_ai::nld_predict::dispatch` (`app/src/ai/agent_providers/active_ai/mod.rs`) and sends it to
the configured model. Nothing looks at the session, so in an ssh session every typing pause is a
model request carrying the buffer. This is the only model request tied to typing pauses: natural
language *detection* itself is a local classifier (`crates/input_classifier`) and sends nothing.

### Chosen fix

- New setting `agents.warp_agent.active_ai.natural_language_autosuggestions_in_ssh_enabled`
  (`NaturalLanguageAutosuggestionsInSshEnabled`, bool, default **false**, not synced, same
  feature flag as its parent). A switch under "Natural Language Autosuggestions" on the AI
  settings page, greyed while the parent switch is off; strings in en, ja, zh-CN.
- The gate is in `nld_predict::dispatch`, the one place every prediction request is built, not in
  `input.rs` (which this task may not edit, and which another agent is changing). `dispatch`
  already receives the pane's `terminal_view_id`; it finds that `TerminalView`
  (`AppContext::view_with_id` per window, a map lookup) and reads its active session: an ssh
  session is `SessionType::WarpifiedRemote` (the ssh wrapper's sessions, and a plain `ssh` the
  user ran that was bootstrapped) or a legacy ssh session. The pane is looked up only when the
  setting is off.
- The decision is a pure function, `allowed_for_session(in_ssh_session: Option<bool>,
  allowed_in_ssh)`. **Unknown is treated as ssh**: if the pane or its active session cannot be
  found, nothing is sent. Not knowing must not leak the buffer.

### Rejected

- **Gate at the trigger in `input.rs`** (`may_send_buffer_as_partial_query`): the natural place,
  and it would also skip the debounce wake-up. Off limits this round. The cost of gating later is
  one 250 ms debounce callback that returns early; no request is built or sent. Recorded for the
  owner of `input.rs`: the same predicate can be added there later to drop the wake-up.
- **A per-pane registry of "in ssh" facts** fed by the terminal view: a second copy of state
  that can go stale. The session is read at the moment of the request instead.
- **Turning off detection (autodetect) in ssh sessions**: it is local, sends nothing, and the
  owner's routing rules depend on it.

### Risk

- If the pane lookup fails for a reason not foreseen (it was not compiled or run), predictions
  stop on local panes too. The lab case's local control catches exactly that: it must see a
  request before the ssh half is judged.
- Next-command suggestions (after a command finishes) and prompt suggestions still run in ssh
  sessions; they are not tied to typing and were not part of the decision. Listed in the report.

### Tests

- Unit: `active_ai/mod_test.rs` `a_query_prediction_is_not_requested_in_an_ssh_session_unless_allowed`
  (local yes; ssh and unknown no; all yes when allowed); `settings/ai_tests.rs`
  `natural_language_autosuggestions_in_ssh_default_off`.
- App: `lab_agent_nld_predict_ssh.bash.lan.nohelper` (agent mode, fake model, in `PROVING_LAB`):
  a partial agent prompt typed on the local pane produces an `nld_predict` request (the fake
  model's own `by_purpose` count); the same on the ssh pane produces none in 5 s. No control
  request makes the case UNVERIFIABLE, never a pass.
