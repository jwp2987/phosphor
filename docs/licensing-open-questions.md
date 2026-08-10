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

## 4. `resources/bundled/mcp_skills/figma/` — no licence, origin unknown

**Status:** unresolved. Bundled into every build.

Eight Figma skills (~35 files of `SKILL.md`, `references/`, `scripts/`) ship
with no licence file and no `license:` frontmatter key. Compare the Anthropic
skills at `resources/bundled/skills/claude-api/` and `.../create-skill/`, which
ship `LICENSE.txt` and cite it from `SKILL.md` frontmatter
(`license: Complete terms in LICENSE.txt`).

The gap is **inherited, not introduced by this fork**: at the pinned upstream
`02b53fcd8`, `git ls-tree -r 02b53fcd8 resources/bundled/` shows the same files
with the same absence, and the only `LICENSE.txt` files under `resources/bundled/`
are the two Anthropic ones. The fork's own history shows the directory arriving
whole in `0dbd3d567` ("Initial public release of Warp").

Whether these were authored by Warp, by Figma, or adapted from a published
Figma skills repository cannot be told from the content.

**What would resolve it:** upstream Warp stating the origin, or a match against
a published Figma skills repository.

## 5. `themes/one_dark.yaml` — source known, licence not

**Status:** unresolved. **Not** bundled — `themes/` is not copied by either
`prepare_bundled_resources` script; these files are installed by hand.

The header records the colour source as `zed-industries/zed`,
`assets/themes/one/one.json`. The zed repository carries several licences at
its root (Apache-2.0, GPL-3.0, AGPL-3.0) applying to different directories, and
which one governs `assets/themes/` has not been verified here. Zed's One theme
itself descends from Atom's One Dark (MIT, GitHub Inc.).

`tokyo_night.yaml` (folke/tokyonight.nvim, MIT) and `vscode_2026_dark.yaml`
(microsoft/vscode source repo, MIT) are recorded in their own headers and are
**not** open questions.

## 6. `app/assets/windows/{x64,arm64}/msvc*.dll` — terms referenced, not reproduced

**Status:** partially resolved.

`app/assets/windows/LICENSE-MSVC-REDIST` identifies the three MSVC
redistributables and points at Microsoft's licence terms and redistribution
list. It does **not** reproduce Microsoft's terms — those are versioned per
Visual Studio release, are not offered under terms permitting verbatim
inclusion in a third-party notice file, and the release these particular
binaries were built from is recorded nowhere in this repository.

**What would resolve it:** recording which Visual Studio release these DLLs
came from, then confirming against Microsoft's redistribution list for that
release that all three are still permitted.

## 7. `crates/warpui/src/rendering/LICENSE-CHROMIUM` — not verbatim

**Status:** placeholder, flagged in the file itself.

Chromium's `LICENSE` could not be fetched offline. The file carries the
canonical SPDX BSD-3-Clause template plus Chromium's copyright line, with a
header saying so. Chromium's actual third clause names Google rather than "the
copyright holder". Replace with the verbatim upstream text.

The same caveat, in milder form, applies to
`app/assets/bundled/syntax_theme/LICENSE-BASE16`: MIT's body text is fixed, but
the copyright line and year were not read from the base16 repository.
