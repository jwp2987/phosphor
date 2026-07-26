# Security Policy

We take security seriously at Phosphor and appreciate the efforts of security researchers who help keep our users safe.

## Reporting a Vulnerability

If you believe you've found a security vulnerability, please follow responsible disclosure practices and **do not** open a public GitHub issue, as this could expose the vulnerability before a fix is available.

Instead, please open a private [GitHub Security Advisory](https://github.com/jwp2987/phosphor/security/advisories/new).

We will acknowledge your report promptly and work with you to understand and resolve the issue as quickly as possible.

## Known limitations / accepted risks

Risks reviewed and consciously accepted (rather than fixed), with rationale.

### Unbounded LLM response / SSE reads (DoS-class) — accepted

- **Where:** `lib/rust-genai/src/webc/web_client.rs` reads the whole response
  body with `res.text().await` (no size cap), and `web_stream.rs` grows the SSE
  `partial_message` buffer until a delimiter arrives (no cap). This is the hot
  path for every BYOP response.
- **Impact:** a malicious or compromised provider endpoint could return an
  arbitrarily large body, or a delimiter-less stream, and exhaust client memory.
- **Why accepted:** this is **stock upstream `genai` behavior**, not a fork
  change, and it is **not fixed upstream** either — verified against the genai
  `main` branch (July 2026); both the body read and the SSE buffer are still
  uncapped there. LLM client libraries generally trust the provider endpoint and
  don't cap the body. The provider is user-configured (semi-trusted) and this is
  a local-first tool, so realistic exposure is low. Upgrading the vendored genai
  would **not** fix it.
- **If revisited:** patch the vendored copy — swap `res.text()` for a
  size-limited `bytes_stream()` read and cap `partial_message` — or send a PR
  upstream. Self-contained (~30 lines) but on every BYOP response, so it needs
  careful testing.
- **Reviewed:** 2026-07-26 parallel security audit.
