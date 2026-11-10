# Linux secure-storage fallback key (#1932): design note, no code yet

Finding 7 of `docs/design/secrets-privacy-review.md` (2026-11-09). Decision for this round: **no
code**. The change is a migration of every credential a keyring-less Linux user has saved, the
gain is small, and a mistake loses those credentials silently. This note is what the code should
follow when the owner schedules it.

## What exists

`crates/warpui_extras/src/secure_storage/linux.rs`. When the Secret Service cannot be used, a
secret is written to `<state_dir>/<service_name>-<key>` as `nonce(12) || AES-256-GCM(value)`,
file mode 0600. The AES key is a string literal compiled into the binary (padded with zeros to 32
bytes). Everything that goes through `SecureStorage::write_value` can land there: BYOP API keys,
agent-provider secrets, MCP OAuth refresh tokens, the proxy password.

So the encryption is obfuscation: anyone who can read the blob and has the (public) binary can
decrypt it. The file mode is the only real protection.

## What a per-user key file buys, honestly

A random 32-byte key in `<state_dir>/secure-storage.key` (0600) sits beside the blobs with the
same owner and mode. An attacker who can read the blobs can read the key.

It does help in three cases:

1. A blob copied without the key file: a backup or sync tool that takes some files, a blob pasted
   into a bug report, a diagnostics bundle that ever picked one up.
2. Offline decryption with nothing but the public binary stops working.
3. The key file can live somewhere stricter than the blobs (see "Where the key lives").

It does not help against another process of the same user, or against root. Only the Secret
Service (or a TPM/kernel-keyring backed key) does. The warning `warn_fallback_backend` already
logs says so and should keep saying so.

## Why not just do it now

- **Silent loss.** A blob that fails to decrypt is reported as `NotFound`. The app then treats
  the key as never saved, and for an API key the next save overwrites the old blob. Any bug in
  the migration, or a key file that goes missing later (a partial restore, a cleaned state
  directory), reads as "your keys are gone" with no error.
- **Two writers.** The GUI and `phosphor-tui` share the state directory, and two app instances
  can start together. Key creation and blob re-encryption need to be atomic and idempotent
  across processes.
- **Downgrade.** v0.1.17 and earlier know only the built-in key. After migration an older build
  reads nothing. Owner call: is that acceptable, and from which version.
- **Shared directory.** The fallback dir is `state_dir()`, shared with unrelated state; the code
  deliberately does not chmod it (see `write_fallback_value`).

## Design

**Format.** New blobs carry a 4-byte magic and a version byte: `PSF1 || nonce(12) || ciphertext`.
A blob without the magic is a legacy blob (built-in key). The format is self-describing, so a
reader never has to guess which key a blob needs, and a later scheme gets `PSF2`.

**Key file.** `secure-storage.key`: 32 random bytes from the OS RNG. Created with
`O_CREAT|O_EXCL`, mode 0600, written to a temporary name in the same directory and `rename`d, so
a second process either creates it or reads the finished file, never half of one. If creation
loses the race (`EEXIST`), read the winner's file. Never regenerate a key file that exists but is
unreadable or the wrong length: report an error and keep using legacy reads; regenerating would
orphan every v1 blob.

**Read.** Magic present: decrypt with the key file's key; failure is an error that is logged
with the key name (never `NotFound`), so a lost key file is visible. No magic: decrypt with the
built-in key, exactly as today.

**Write.** Always v1, once a key file exists or can be created. If the key file cannot be
created (read-only state dir), write legacy and log once.

**Migration.** Lazy, on a successful legacy read: write the v1 blob to a temporary file, fsync,
`rename` over the legacy blob. The legacy blob is replaced only by a blob that has just been
decrypted back and compared with the value read. No bulk pass at startup: reads already happen
at startup for every credential in use, and a credential that is never read is never at risk.

**Where the key lives.** Start with the state directory, for simplicity. A follow-up can move it
to the kernel keyring (`keyctl`, user keyring) where available, which does separate the key from
the files; that is a second decision, not part of the first change.

**The built-in key stays in the binary** for legacy reads for at least two releases, with its
"do not rename" comment.

## Tests the change needs

All in `secure_storage/linux_test.rs` with a temp fallback dir and the keyring seam returning
"unavailable":

1. Round trip with a fresh key file; the blob starts with the magic; the key file is 0600.
2. A legacy blob (written with the built-in key) is read, then found rewritten as v1, and reads
   again; the value is identical.
3. Two `SecureStorage` instances on one directory, created concurrently (threads): one key file,
   both read each other's blobs.
4. Key file deleted after a v1 write: the read is an error that is not `NotFound`, the blob is
   untouched, and no new key file replaces the old one on that read.
5. Key file of the wrong length: same as 4.
6. Read-only directory: writes fall back to legacy and still round trip.
7. A v1 blob with a flipped byte fails authentication and is reported as an error.

## Owner decisions

1. Do it at all, given the gain is cases 1-3 above and not protection from local processes?
2. Is losing fallback credentials on a downgrade to v0.1.17 or earlier acceptable?
3. Kernel keyring as the key's home: now, later, or never?
