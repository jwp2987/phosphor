# Licensing — open questions

Attribution questions that are **not resolved**. Everything listed here ships in
the product but has no determinable licence or provenance, so no attribution has
been written for it. Nothing here should be given an invented licence — a wrong
attribution is worse than a recorded gap.

Resolved attribution lives in the code (per-file headers and `LICENSE-*` files)
and in the `ADDITIONAL_LICENSES` manifests in
`script/prepare_bundled_resources` and
`script/windows/prepare_bundled_resources.ps1`.

---

## 1. `app/assets/bundled/fonts/password.ttf` — unknown

**Status:** unresolved. Ships in every build.

The file carries no licence, no copyright string and no provenance note. It has
been present since Warp's first public commit, so the fork's history contains no
record of where it came from. It is used to render obscured (password) input.

**What would resolve it:** the upstream Warp maintainers stating its origin, or
identifying the typeface from its glyph tables. Until then, no attribution can
be written.

**Risk if wrong:** a redistributed font with incompatible terms. Consider
replacing it with a known-licensed font (or a drawn substitute) if the origin
cannot be established.

## 2. `app/assets/bundled/svg/*.svg` — the ~356-icon UI set — unknown

**Status:** unresolved. Ships in every build.

The general UI icon set at the top level of `app/assets/bundled/svg/`
(~356 files: `alert-circle.svg`, `arrow-narrow-down.svg`, `bar-chart-04.svg`, …)
carries **no licence marker in any file** — no `<!-- -->` comment, no `<metadata>`
block, no `<title>` crediting a source.

The naming convention (`alert-hexagon`, `arrow-block-left`, `bar-chart-04`,
`arrow-circle-broken-up`) matches the **Untitled UI Icons** set, whose numbered
variants and hyphenated compound names are distinctive. That is a *suggestion
from naming only* — it is not evidence, and no file confirms it. Untitled UI
Icons are distributed under terms that differ by tier, so even a confirmed match
would not by itself settle whether redistribution here is permitted.

**Excluded from this entry:** `app/assets/bundled/svg/file_type/` — see the
separate SVG Repo attribution, which is documented and manifest-listed.

**What would resolve it:** upstream Warp confirming the source set and the tier
purchased, or a diff against a published Untitled UI release.

## 3. `app/assets/bundled/svg/file_type/*.svg` — attributed, licence unverified

**Status:** partially resolved — attribution recorded, licence *not* determined.

17 of the 20 icons carry `Uploaded to: SVG Repo, www.svgrepo.com`. SVG Repo is a
*redistributor*: the per-icon licence depends on the originating set, and the
comment does not record it. Several of the affected files also carry
`<title>file_type_go</title>`-style titles matching the **vscode-icons** project
(MIT), which would make them MIT — but that is inference from a title string,
not a licence statement in the file.

`app/assets/bundled/svg/file_type/ATTRIBUTION.md` records the source and the
affected filenames. It deliberately does **not** assert a licence.

**What would resolve it:** looking each icon up on svgrepo.com and recording the
licence it is published under there.
