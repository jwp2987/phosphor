#!/usr/bin/env python3
# Extract docs/sweep/*.md's hand-adjudicated pin-test verdicts into the
# machine-readable ledger docs/sweep-verdict-ledger.tsv.
#
# WHY THIS EXISTS
# ----------------
# 1,841 pin tests were hand-adjudicated at pin 02b53fcd8 across six per-area
# prose files (docs/sweep/*.md), written by six different agents in six
# slightly different Markdown conventions. That work cost a full session and
# is not repeatable on demand -- it required reading pin source, reading fork
# source, and often reading DECLINED.md/SCOPE-*.md for cross-references. When
# the pin moves to N+1, none of that reasoning should be redone for a test
# whose file and DECLINED.md citation haven't changed. See docs/SWEEP-SUMMARY.md
# for the sweep's own results and docs/ORACLE.md for the re-pin policy.
#
# This script is what turned the prose into the TSV once. It is NOT part of
# the ongoing re-pin flow -- script/generate_repin_queue and
# script/check_sweep_ledger consume the TSV, not the prose. Re-run this
# script only if a bug is found in ITS extraction (not to re-adjudicate a
# test -- that's a docs/sweep-verdict-ledger.tsv edit, made by hand, citing
# the same evidence conventions as existing rows).
#
# HOW IT WORKS, AND WHY IT IS ONLY "MOSTLY" MECHANICAL
# ------------------------------------------------------
# docs/SWEEP-INVENTORY.md is the ground-truth NAME REGISTRY: every pin test
# name, grouped by pin file, from the mechanical inventory pass that preceded
# hand adjudication. It is far more complete and uniform than the six
# per-area docs (which often summarize a bucket by wildcard or "all N",
# without spelling out every name), so extraction works by:
#
#   1. Parse the registry: {pin_file: [test_name, ...]}.
#   2. Parse each per-area doc's own per-file sections for verdicts, using
#      three techniques depending on what the doc actually did (all three
#      appear, sometimes within the SAME doc):
#        a. table rows (`| test | verdict | evidence |`) -- the cleanest,
#           used throughout warp-tui.md and inside a few app-terminal.md
#           subsections.
#        b. bullet lists (`- **BUCKET** — evidence` then indented
#           `  - \`test_name\`` lines) -- app-ai.md's format throughout.
#        c. prose paragraphs naming a bucket in **bold** and some backtick
#           test names nearby, with NO fixed structure -- crates-ai.md,
#           warp-cli.md, settings-workspace.md, and app-terminal.md's
#           per-file table (file-level only, no per-test names for most
#           rows).
#   3. Cross-validate every extracted (test, bucket) pair against the
#      registry for that pin file: a name the doc's prose mentions that
#      ISN'T a real absent test for that file (usually a symbol/method name
#      quoted as evidence, not a test) is silently dropped as noise, not
#      recorded as a verdict.
#   4. Where a doc names only SOME of a file's tests and states a bucket
#      count for the rest ("**CLOUD** (11): <no names>. **DECLINED** (1):
#      `named_test`."), the unnamed remainder is inferred IF exactly one
#      bucket's stated count matches the unresolved count exactly (confidence
#      "judgement", not "clean" -- see below).
#   5. Whatever remains genuinely unresolved is recorded with verdict
#      UNPARSED, never silently dropped and never guessed past the point the
#      source document itself commits to an answer.
#
# A handful of files needed outright hand transcription (HAND_TERM below,
# and the crates-ai.md/pane_group special-casing) because their prose uses a
# structure no general rule could safely cover -- e.g. citing fork-side
# renamed test names instead of the pin-side names the registry uses, or
# `grok_*`/`geap_*` substring families instead of an exact list. Each is
# commented at its use site with how it was verified (usually: the resulting
# counts reconcile exactly against the doc's own stated arithmetic).
#
# CONFIDENCE LEVELS (the `confidence` column)
# ---------------------------------------------
#   clean      -- the source doc named this exact test under this exact
#                 bucket (bullet, table row, or unambiguous "all N" claim).
#   judgement  -- resolved via inference this script documents (count-hint
#                 remainder, glob/substring family, fork-renamed name
#                 cross-match, or hand transcription of an irregular
#                 subsection). Verified to reconcile against the source
#                 doc's own stated totals, but not a literal per-test
#                 citation.
#   unparsed   -- genuinely left unresolved. Mostly the settings-workspace.md
#                 pane_group/mod_tests.rs "needs a second look, not
#                 re-verified this pass" tests, which the ORIGINAL sweep
#                 itself declined to bucket -- this script does not invent a
#                 verdict the source document didn't commit to.
#
# Report printed on every run: per-doc extraction counts against each doc's
# own declared total, per-file reconciliation against the registry, and a
# final cross-area duplicate check. Four of the six docs' own top-of-file
# summary tables have small internal arithmetic bugs (their stated bucket
# totals don't sum to their own per-file section headers) -- this script
# trusts the per-file sections, which is what each doc itself says to do
# when it notices the same thing about its own arithmetic.
#
# USAGE
#     script/extract_sweep_ledger.py
#         Regenerates docs/sweep-verdict-ledger.tsv and prints the
#         reconciliation report to stdout.

import re
import os
from collections import Counter, OrderedDict, defaultdict

REPO_ROOT = os.path.normpath(os.path.join(os.path.dirname(__file__), ".."))


def read(path):
    with open(os.path.join(REPO_ROOT, path), encoding="utf-8") as f:
        return f.read()


BUCKETS = {
    "CLOUD", "DECLINED", "DIVERGENT", "MISSING-SUBSYSTEM", "COVERED-ELSEWHERE",
    "PORTABLE", "PORTED", "DEFECT-FIXED", "PORTABLE-OUT-OF-AREA", "FORK-AHEAD",
}

BOLD_BUCKET_RE = re.compile(r'\*\*(\d+\s+)?([A-Za-z][^*]*?)\*\*')
BACKTICK_RE = re.compile(r'`([a-z_][a-z0-9_]*)`')


# --------------------------------------------------------------- registry --
def parse_inventory(text):
    """SWEEP-INVENTORY.md -> {pin_file: {"absent": N, "tests": [name, ...]}}.

    This is the ground-truth name registry every per-area doc's verdicts are
    cross-checked against."""
    registry = OrderedDict()
    for sec in re.split(r'\n(?=### `)', text):
        m = re.match(r'### `([^`]+)` — (\d+) absent', sec)
        if not m:
            continue
        fpath, absent = m.group(1), int(m.group(2))
        names = re.findall(r'^\s*-\s*`([^`]+)`', sec, re.M)
        registry[fpath] = {"absent": absent, "tests": names}
    return registry


def normalize_bucket(raw):
    """Best-effort: pick the PRIMARY bucket out of a verdict cell/label that
    may carry bold markers, a leading count, a trailing parenthetical
    annotation, or be a compound ("A (N) + B (M)", "A / B", "A, inert").
    Always returns the first bucket named, or None if nothing recognisable
    is found."""
    raw = raw.strip()
    bm = re.search(r'\*\*([^*]+?)\*\*', raw)
    candidate = bm.group(1) if bm else raw
    for sep in (' + ', ' / '):
        if sep in candidate:
            candidate = candidate.split(sep)[0]
            break
    if ',' in candidate:
        candidate = candidate.split(',')[0]
    candidate = re.sub(r'\(.*?\)', '', candidate)
    candidate = re.sub(r'^\d+\s+', '', candidate)
    candidate = candidate.strip(' *.:;,')
    if candidate in BUCKETS:
        return candidate
    # "4 already PORTED", "newly CLOUD" etc -- find a bucket token anywhere
    for tok in candidate.split():
        if tok in BUCKETS:
            return tok
    # "DIVERGENT-by-decision", "MISSING-SUBSYSTEM, inert" (comma already
    # split above, but a hyphen suffix like "-by-decision" survives to here)
    for b in BUCKETS:
        if candidate.startswith(b) and (len(candidate) == len(b) or not candidate[len(b)].isalnum()):
            return b
    return None


def find_table_rows(sec_text):
    """Extract (test, verdict, evidence) from a '| test | verdict | evidence |'
    Markdown table anywhere in sec_text."""
    out = []
    in_table = False
    for line in sec_text.splitlines():
        if re.match(r'\|\s*test\s*\|\s*verdict\s*\|', line, re.I):
            in_table = True
            continue
        if in_table:
            if re.match(r'\|\s*-+\s*\|', line):
                continue
            m = re.match(r'\|\s*`([^`]+)`\s*\|\s*(.+?)\s*\|\s*(.*?)\s*\|\s*$', line)
            if m:
                test, verdict_raw, evidence = m.groups()
                verdict = normalize_bucket(verdict_raw) or normalize_bucket('**' + verdict_raw + '**')
                out.append((test, verdict, evidence[:220]))
            elif line.strip() == '' or not line.strip().startswith('|'):
                in_table = False
    return out


def generic_bucket_spans(sec_text, file_names):
    """Nearest-preceding-bold-bucket extraction: for every recognised bucket
    label in sec_text, everything up to the NEXT recognised bucket label is
    its evidence span, and every backtick token in that span that is also a
    real absent-test name for this file (per file_names) is attributed to
    it. Over-capturing is safe -- unrelated names get filtered by the
    file_names membership check, not attributed wrongly."""
    out = []
    bucket_positions = []
    for m in BOLD_BUCKET_RE.finditer(sec_text):
        b = normalize_bucket(m.group(2))
        if b:
            bucket_positions.append((m.start(), m.end(), b))
    for i, (start, end, bucket) in enumerate(bucket_positions):
        span_end = bucket_positions[i + 1][0] if i + 1 < len(bucket_positions) else len(sec_text)
        span = sec_text[end:span_end]
        evidence = re.sub(r'\s+', ' ', span).strip()[:220]
        for tm in BACKTICK_RE.finditer(span):
            name = tm.group(1)
            if name in file_names:
                out.append((name, bucket, evidence))
    return out


# ------------------------------------------------------------- app-ai.md ---
def parse_app_ai(text):
    """app-ai.md's format: clean '### `file` — N absent' sections, each a
    flat list of '- **BUCKET** — evidence' bullets followed by indented
    '  - `test_name`' lines. Extracts 917/917 cleanly -- no fallback needed."""
    m = re.search(r'\n## Per-file adjudication\n(.*?)\n## ', text, re.S)
    body = m.group(1)
    rows = []
    for sec in re.split(r'\n(?=### `)', body):
        hm = re.match(r'### `([^`]+)` — (\d+) absent', sec)
        if not hm:
            continue
        fpath = hm.group(1)
        cur_bucket, cur_evidence = None, ""
        for line in sec.splitlines():
            bm = re.match(r'-\s+\*\*([A-Za-z-]+)\*\*\s*—\s*(.*)', line)
            if bm:
                cur_bucket, cur_evidence = bm.group(1), bm.group(2).strip()
                continue
            tm = re.match(r'\s+-\s+`([^`]+)`', line)
            if tm and cur_bucket:
                rows.append({
                    "test": tm.group(1), "pin_file": fpath, "verdict": cur_bucket,
                    "evidence": cur_evidence[:220], "confidence": "clean", "area": "app-ai",
                })
    return rows


# ----------------------------------------------------------- warp-tui.md ---
def parse_warp_tui(text, inv):
    """warp-tui.md mostly uses per-test tables, with a handful of files
    stated as a whole-file "**VERDICT**, all N" prose verdict instead."""
    tui_prefix = "crates/warp_tui/src/"
    lookup = {f[len(tui_prefix):]: f for f in inv if f.startswith(tui_prefix)}
    m = re.search(r'\n## Per-file adjudication\n(.*?)\n## ', text, re.S)
    body = m.group(1)
    rows, report = [], []
    for sec in re.split(r'\n(?=### `)', body):
        hm = re.match(r'### `([^`]+)` — (\d+) absent', sec)
        if not hm:
            continue
        rel, absent = hm.group(1), int(hm.group(2))
        full = lookup.get(rel)
        if not full:
            report.append((rel, "NO REGISTRY MATCH", 0, absent))
            continue
        file_names = set(inv[full]["tests"])
        got = {}
        for test, verdict, evidence in find_table_rows(sec):
            if verdict and test in file_names:
                got[test] = (verdict, evidence, "clean")
        for name, bucket, evidence in generic_bucket_spans(sec, file_names):
            if name not in got:
                got[name] = (bucket, evidence, "clean")
        missing = file_names - set(got)
        if missing:
            head_text = sec[:400]
            wm = re.search(r'\*\*([A-Za-z][A-Za-z -]*?)\*\*,?\s*(all \d+|both)', head_text)
            if wm:
                fb = normalize_bucket(wm.group(1))
                if fb:
                    ev = re.sub(r'\s+', ' ', head_text).strip()[:220]
                    for name in list(missing):
                        got[name] = (fb, ev, "clean")
                    missing = file_names - set(got)
        sec_ref = None
        rm = re.search(r'#(\d+)', sec)
        if rm:
            sec_ref = f"#{rm.group(1)}"
        for name, (verdict, evidence, conf) in got.items():
            if verdict == "DECLINED" and not re.search(r'#\d', evidence) and sec_ref:
                evidence = f"{evidence} (section cites {sec_ref})"
            rows.append({"test": name, "pin_file": full, "verdict": verdict,
                         "evidence": evidence, "confidence": conf, "area": "warp-tui"})
        for name in missing:
            rows.append({"test": name, "pin_file": full, "verdict": "UNPARSED",
                         "evidence": "not resolved by table/prose/whole-file extraction",
                         "confidence": "unparsed", "area": "warp-tui"})
        report.append((rel, full, len(got), absent))
    return rows, report


# ------------------------------------------------------- app-terminal.md ---
# app-terminal.md's "## Per-file verdicts" section is a single Markdown TABLE
# (file, absent count, verdict, evidence) -- no per-test names for
# single-verdict files (the whole-file verdict + registry cross-reference is
# enough there). A handful of files are stated MIXED/compound in the table
# and only broken down per-test in irregular prose subsections further down
# the doc; those are hand-transcribed here, each verified against the
# registry's exact name list for that file (see the comment on each group).
TERM_PREFIX = "app/src/terminal/"
HAND_TERM = [
    # model/terminal_model_tests.rs -- "the 11, individually"
    ("model/terminal_model_tests.rs", "cloud_mode_deferred_terminal_model_starts_view_pending", "PORTED",
     "ported as ambient_agent_deferred_terminal_model_starts_view_pending (branding rename)"),
    ("model/terminal_model_tests.rs", "generic_shared_session_viewer_model_starts_view_pending", "PORTED",
     "unchanged from pin, TerminalModel::new_for_shared_session_viewer already exists"),
    ("model/terminal_model_tests.rs", "precmd_with_completion_metadata_records_completion_mismatch_without_overwriting_completed_block", "PORTED",
     "ported, dropped Event::LifecycleRecovery assertion (telemetry sending physically removed)"),
    ("model/terminal_model_tests.rs", "precmd_with_completion_metadata_recovers_in_band_completion_and_reuses_cached_prompt", "PORTED",
     "unchanged from pin"),
    ("model/terminal_model_tests.rs", "repeated_precmd_with_completion_metadata_and_prompt_only_precmd_are_ignored", "PORTED",
     "recovery-enabled variant; same LifecycleRecovery adaptation"),
    ("model/terminal_model_tests.rs", "is_cloud_agent_conversation_only_true_for_genuine_ambient_sessions", "DIVERGENT",
     "fork's SessionSourceType::User has no task-id field; the exact leak scenario cannot be constructed"),
    ("model/terminal_model_tests.rs", "cloud_mode_setup_phase_ended_emits_when_sharing", "CLOUD",
     "needs send_cloud_mode_setup_phase_ended_for_shared_session()/CloudModeSetupPhaseEnded, dropped FeatureFlag::CloudMode"),
    ("model/terminal_model_tests.rs", "cloud_mode_setup_phase_ended_does_not_emit_when_not_sharing", "CLOUD",
     "same dropped FeatureFlag::CloudMode/CloudModeSetupV2 mechanism"),
    ("model/terminal_model_tests.rs", "sharer_rejects_dcs_hook_with_unregistered_session_id", "MISSING-SUBSYSTEM",
     "CommandFinishedValue/PreexecValue/BootstrappedValue have no session_id field; documented DCS session-registration gap"),
    ("model/terminal_model_tests.rs", "viewer_processes_dcs_hook_with_unregistered_session_id", "MISSING-SUBSYSTEM",
     "same DCS session-registration gap"),
    ("model/terminal_model_tests.rs", "ssh_bootstraps_if_blocklist_empty_and_reconciles_parent_return", "MISSING-SUBSYSTEM",
     "same DCS session-registration gap"),
    # input/slash_commands/mod_tests.rs -- table row body text gives 8 family
    # fragments + 1 named DECLINED test; test names cross-matched against the
    # inventory registry rather than the truncated table prose.
    ("input/slash_commands/mod_tests.rs", "auto_approve_is_an_exact_no_argument_command", "CLOUD",
     "needs settings::SettingsMode (documented dropped) or cloud gating"),
    ("input/slash_commands/mod_tests.rs", "cloud_mode_v2_commands_are_active_only_in_cloud_mode_v2_context", "CLOUD",
     "cloud_mode_v2 family"),
    ("input/slash_commands/mod_tests.rs", "exit_command_executes_immediately_and_takes_no_argument", "CLOUD",
     "needs settings::SettingsMode (documented dropped) or cloud gating"),
    ("input/slash_commands/mod_tests.rs", "natural_language_detection_command_is_supported_in_tui", "CLOUD",
     "needs settings::SettingsMode (documented dropped) or cloud gating"),
    ("input/slash_commands/mod_tests.rs", "not_cloud_agent_commands_are_only_active_outside_cloud_mode", "CLOUD",
     "not_cloud_agent family"),
    ("input/slash_commands/mod_tests.rs", "slash_command_is_submitted_as_prompt_only_for_prompt_commands", "CLOUD",
     "needs settings::SettingsMode (documented dropped) or cloud gating"),
    ("input/slash_commands/mod_tests.rs", "theme_command_inserts_input_for_its_required_argument", "CLOUD",
     "theme family, needs settings::SettingsMode"),
    ("input/slash_commands/mod_tests.rs", "tui_commands_have_typed_identities_and_explicit_surface_support", "CLOUD",
     "tui_commands family"),
    ("input/slash_commands/mod_tests.rs", "logout_command_executes_immediately_and_takes_no_argument", "DECLINED",
     "existing /logout DECLINED row, #338"),
    # cli_agent_sessions/listener/mod_tests.rs -- "the 3, individually"
    ("cli_agent_sessions/listener/mod_tests.rs", "codex_try_parse_ignores_structured_event_without_codex_plugin", "DEFECT-FIXED",
     "CodexSessionHandler::try_parse never gated structured Codex events on FeatureFlag::CodexPlugin; fixed and ported this sweep"),
    ("cli_agent_sessions/listener/mod_tests.rs", "oh_my_pi_end_to_end_parsing_and_handling", "COVERED-ELSEWHERE",
     "already ported as omp_end_to_end_parsing_and_handling (CLIAgent::Omp, not OhMyPi, #273)"),
    ("cli_agent_sessions/listener/mod_tests.rs", "oh_my_pi_is_supported", "COVERED-ELSEWHERE",
     "already ported as omp_is_supported, same rename"),
    # view/use_agent_footer/mod_tests.rs -- "the branding-rename finding"
    ("view/use_agent_footer/mod_tests.rs", "test_rich_input_submit_strategy_for_oh_my_pi", "COVERED-ELSEWHERE",
     "already ported verbatim as omp_uses_bracketed_paste_submission"),
    ("view/use_agent_footer/mod_tests.rs", "cli_agent_footer_does_not_render_for_warp_tui_session", "COVERED-ELSEWHERE",
     "-> cli_agent_footer_does_not_render_for_phosphor_tui_session"),
    ("view/use_agent_footer/mod_tests.rs", "cli_agent_footer_renders_for_viewer_of_shared_cloud_agent_session", "COVERED-ELSEWHERE",
     "-> cli_agent_footer_renders_for_viewer_of_shared_ambient_agent_session"),
    ("view/use_agent_footer/mod_tests.rs", "use_agent_footer_hidden_during_cloud_agent_setup_lrc", "COVERED-ELSEWHERE",
     "-> use_agent_footer_hidden_during_ambient_agent_setup_lrc, body-diffed identical"),
    ("view/use_agent_footer/mod_tests.rs", "insert_cli_agent_voice_text_hermes_multiline_uses_bracketed_paste_without_submitting", "DECLINED",
     "Voice input, DECLINED.md #389/#352"),
    # warpify/settings_tests.rs -- "the split" (one pin test, split verdict:
    # half its own assertion is PORTED, half is DECLINED -- recorded as a
    # single MIXED row rather than invented as two test names).
    ("warpify/settings_tests.rs", "test_deprecated_ssh_wrapper_migration_triggers_are_not_synced", "MIXED",
     "asserts 2 things: half PORTED as enable_ssh_wrapper_migration_trigger_is_not_synced; half DECLINED, extends #322 (SSH tmux wrapper kept)"),
]


def parse_app_terminal(text, inv):
    m = re.search(r'\n## Per-file verdicts\n(.*?)\n## ', text, re.S)
    term_table = m.group(1)
    hand_by_file = defaultdict(list)
    for rel, test, verdict, ev in HAND_TERM:
        hand_by_file[rel].append((test, verdict, ev))

    rows, report = [], []
    for line in term_table.splitlines():
        m2 = re.match(r'\|\s*`([^`]+)`\s*\|\s*(\d+)\s*\|\s*(.+?)\s*\|\s*(.*?)\s*\|\s*$', line)
        if not m2:
            continue
        rel, absent, verdict_cell, evidence = m2.groups()
        absent = int(absent)
        full = TERM_PREFIX + rel
        if full not in inv:
            report.append((rel, "NO REGISTRY MATCH", 0, absent))
            continue
        file_names = set(inv[full]["tests"])
        got = {}
        if rel in hand_by_file:
            for test, verdict, ev in hand_by_file[rel]:
                if test in file_names:
                    got[test] = (verdict, ev, "judgement")
        else:
            v = normalize_bucket(verdict_cell)
            if v:
                for name in file_names:
                    got[name] = (v, evidence[:220], "clean")
        missing = file_names - set(got)
        full_ref = None
        rm = re.search(r'#(\d+)', verdict_cell + " " + evidence)
        if rm:
            full_ref = f"#{rm.group(1)}"
        for name, (verdict, ev, conf) in got.items():
            if verdict == "DECLINED" and not re.search(r'#\d', ev) and full_ref:
                ev = f"{ev} (row cites {full_ref})"
            rows.append({"test": name, "pin_file": full, "verdict": verdict,
                         "evidence": ev, "confidence": conf, "area": "app-terminal"})
        for name in missing:
            rows.append({"test": name, "pin_file": full, "verdict": "UNPARSED",
                         "evidence": f"table verdict cell was: {verdict_cell}",
                         "confidence": "unparsed", "area": "app-terminal"})
        report.append((rel, full, len(got), absent))
    return rows, report


# ---------------------------------------------- generic prose (3 docs) -----
# crates-ai.md, warp-cli.md, and settings-workspace.md have no fixed
# structure -- a per-file "### `path` — N absent [→|—] VERDICT" header,
# then prose that sometimes names every test, sometimes names a bucket's
# count without spelling out its names, and sometimes states the whole
# file's verdict once ("**CLOUD**, all 6.") without repeating it per test.
def parse_prose_doc(text, area, inv, bounds_re):
    m = re.search(bounds_re, text, re.S)
    body = m.group(1)
    rows, report = [], []
    for sec in re.split(r'\n(?=### )', body):
        hm = re.match(r'### `([^`]+)` — (\d+) absent', sec)
        if not hm:
            continue
        full = hm.group(1)
        absent = int(hm.group(2))
        if full not in inv:
            report.append((full, "NO REGISTRY MATCH", 0, absent))
            continue
        file_names = set(inv[full]["tests"])
        got = {}
        for test, verdict, evidence in find_table_rows(sec):
            if verdict and test in file_names:
                got[test] = (verdict, evidence, "clean")
        for name, bucket, evidence in generic_bucket_spans(sec, file_names):
            if name not in got:
                got[name] = (bucket, evidence, "clean")
        missing = file_names - set(got)
        if missing:
            # "**CLOUD**, all 6." / "**CLOUD** (all 9)" -- an explicit,
            # unambiguous total-coverage claim naming the file's FULL
            # declared absent count. Checked first (stronger evidence than
            # the subset heuristics below) and applies regardless of which
            # subset `missing` currently is.
            head = sec[:400]
            am = re.search(r'\*\*([A-Za-z][A-Za-z -]*?)\*\*,?\s*(?:\(all\s+(\d+)\)|all\s+(\d+))', head)
            if am:
                b = normalize_bucket(am.group(1))
                n = int(am.group(2) or am.group(3))
                if b and n == absent:
                    ev = re.sub(r'\s+', ' ', head).strip()[:220]
                    for name in list(missing):
                        got[name] = (b, ev, "judgement" if len(got) else "clean")
                    missing = file_names - set(got)
            if missing == file_names:
                # whole-file fallback: the header line's own "→ VERDICT" (or a
                # bold verdict early in the section) covers the entire file.
                head = sec[:400]
                fb = None
                hb = re.search(r'—\s*(\d+)\s*absent[^\n]*?(?:→|—)\s*\*{0,2}([A-Za-z][A-Za-z -]*?)\*{0,2}\s*$',
                                sec.splitlines()[0])
                if hb:
                    fb = normalize_bucket(hb.group(2))
                if not fb:
                    bm = BOLD_BUCKET_RE.search(head)
                    if bm:
                        fb = normalize_bucket(bm.group(2))
                if fb:
                    ev = re.sub(r'\s+', ' ', head).strip()[:220]
                    for name in list(missing):
                        got[name] = (fb, ev, "clean")
                    missing = file_names - set(got)
            elif missing:
                # partial split, e.g. "**CLOUD** (11): <prose, no names>.
                # **DECLINED** (1): `named_test`." -- the named bucket(s)
                # resolve via generic_bucket_spans above; the bucket with an
                # explicit "(N)" count and ZERO resolved names is the one
                # whose names were never spelled out. Assign the remainder to
                # it only when exactly one such bucket exists and its stated
                # count matches len(missing) exactly.
                count_hints = {}
                for cm in re.finditer(r'\*\*([A-Za-z][A-Za-z -]*?)\*\*\s*\((\d+)\)', sec):
                    b = normalize_bucket(cm.group(1))
                    if b:
                        count_hints[b] = int(cm.group(2))
                for cm in re.finditer(r'\*\*(\d+)\s+([A-Za-z][A-Za-z -]*?)\*\*', sec):
                    b = normalize_bucket(cm.group(2))
                    if b:
                        count_hints[b] = int(cm.group(1))
                resolved_counts = Counter(v[0] for v in got.values())
                candidates = [b for b, n in count_hints.items()
                              if n == len(missing) and resolved_counts.get(b, 0) == 0]
                if len(candidates) == 1:
                    b = candidates[0]
                    ev = (f"whole remainder inferred: section named {b} ({len(missing)}) "
                          f"without spelling out individual test names")
                    for name in list(missing):
                        got[name] = (b, ev, "judgement")
                    missing = file_names - set(got)
        # Ref-search over the FULL section text, not just the (possibly
        # truncated) per-row evidence string: several DECLINED verdicts are
        # resolved via the whole-file/all-N fallback using only the first
        # ~400 chars of the section as evidence, while the actual "#NNN"
        # citation sits later in the same section's prose.
        sec_ref = None
        rm = re.search(r'#(\d+)', sec)
        if rm:
            sec_ref = f"#{rm.group(1)}"
        for name, (verdict, evidence, conf) in got.items():
            if verdict == "DECLINED" and not re.search(r'#\d', evidence) and sec_ref:
                evidence = f"{evidence} (section cites {sec_ref})"
            rows.append({"test": name, "pin_file": full, "verdict": verdict,
                         "evidence": evidence, "confidence": conf, "area": area})
        for name in missing:
            rows.append({"test": name, "pin_file": full, "verdict": "UNPARSED",
                         "evidence": "not resolved by table/prose/whole-file extraction",
                         "confidence": "unparsed", "area": area})
        report.append((full, full, len(got), absent))
    return rows, report


# --------------------------------------------------------------- specials --
def resolve_crates_ai_specials(rows_by_file, inv):
    """Two crates-ai.md files need hand resolution beyond the generic parser:

    api_keys_tests.rs (55 tests): the doc's own header-comment breakdown
    ("24 CLOUD -- grok_*, has_grok_subscription_*, two manager_has_any_key_*
    cases", "12 CLOUD -- geap_*", "19 DECLINED -- the rest") names FAMILIES by
    wildcard, not exact per-test names for most of the 24+12. Verified by
    hand: substring "grok" matches exactly 24 registry names for this file,
    substring "geap" matches exactly 12, and the remaining 19 (neither
    substring) are exactly the custom_endpoint/custom_model_providers
    DECLINED group the doc names explicitly. 24+12+19=55 reconciles the
    doc's own arithmetic, so this is judgement, not a guess.

    project_context/model_tests.rs (6 tests): header verdict is the compound
    "DIVERGENT / MISSING-SUBSYSTEM" (the generic whole-file fallback can't
    parse a "/"-joined header), and the body text's own final call is
    "MISSING-SUBSYSTEM, correctly flagged non-cloud in the fork's own
    comment" for all 6 -- so MISSING-SUBSYSTEM is the primary verdict."""
    f = "crates/ai/src/api_keys_tests.rs"
    if f in inv:
        for name in inv[f]["tests"]:
            if name in rows_by_file.get(f, {}):
                continue
            if "grok" in name:
                rows_by_file.setdefault(f, {})[name] = (
                    "CLOUD",
                    "xAI/Grok subscription OAuth token family (substring match; DECLINED.md #319 is "
                    "the *subscription-flow* row -- these are the CLOUD token-plumbing tests, not "
                    "#319's own DECLINED tests)",
                    "judgement")
            elif "geap" in name:
                rows_by_file.setdefault(f, {})[name] = (
                    "CLOUD", "GEAP managed-secrets token family (substring match)", "judgement")
            else:
                rows_by_file.setdefault(f, {})[name] = (
                    "DECLINED", "custom_endpoint/custom_model_providers family, DECLINED.md #142/#347",
                    "judgement")
    f2 = "crates/ai/src/project_context/model_tests.rs"
    if f2 in inv:
        for name in inv[f2]["tests"]:
            if name in rows_by_file.get(f2, {}):
                continue
            rows_by_file.setdefault(f2, {})[name] = (
                "MISSING-SUBSYSTEM",
                "path_to_rules/ProjectRule::path have no HostId dimension; global_rules.rs (#575) "
                "already solved the same problem for global rules",
                "judgement")
    return rows_by_file


# app/src/pane_group/mod_tests.rs (33 tests, settings-workspace.md): the
# doc's own breakdown uses approximate counts ("~26", "~5") and glob patterns
# including a "(x3)" multiplier, plus a 5-test "needs a second look, not
# re-verified this pass" bullet the doc itself declines to bucket. Resolved
# by hand against the inventory's 33 exact names; arithmetic reconciles
# exactly (21 CLOUD + 5 DIVERGENT + 2 DECLINED + 5 explicitly-unresolved = 33).
PG_FILE = "app/src/pane_group/mod_tests.rs"
PG_DIVERGENT = {
    "test_add_pane_restores_hidden_child_when_parent_is_already_fullscreen",
    "test_ensure_hidden_child_agent_pane_materializes_missing_child_pane",
    "test_ensure_hidden_child_agent_pane_materializes_restored_remote_child_linked_by_parent_agent_id",
    "test_ensure_hidden_child_agent_pane_skips_child_owned_by_another_pane_group",
    "test_hidden_child_creation_applies_ambient_task_id_to_controller",
}
PG_DECLINED = {"test_start_shared_session_from_modal", "test_stop_shared_session"}
PG_UNRESOLVED = {
    "test_reattach_panes_restores_hidden_child_when_parent_is_already_fullscreen",
    "test_restore_closed_pane_restores_hidden_child_when_parent_is_already_fullscreen",
    "test_replace_pane_restores_hidden_child_when_replacement_is_already_fullscreen",
    "test_pane_group_restore_loop_keeps_orchestration_topology_and_materializes_child_pane",
    "test_swapping_to_child_agent_from_maximized_pane_keeps_maximized_state",
}
# settings_view/mod_tests.rs (9 tests): the doc names 2 exception tests
# BEFORE the bold "**COVERED-ELSEWHERE**" label that describes them, which
# the forward-only bucket-span heuristic misattributes to the file's
# preceding blanket "CLOUD, all 9" instead. Corrected by hand.
SW_COVERED_FIX = {
    "code_subpages_are_identified": "ai_subpages_are_identified (line 21)",
    "code_subpages_map_to_code_backing_page": "ai_subpages_map_to_ai_backing_page (line 52)",
}


def apply_settings_workspace_specials(rows):
    rows = [r for r in rows if r["pin_file"] != PG_FILE]
    for name in INV[PG_FILE]["tests"]:
        if name in PG_DIVERGENT:
            verdict, ev = "DIVERGENT", (
                "depends on lazy hidden-child-agent-pane restoration "
                "(restore_missing_child_agent_panes_for_parent) the fork lacks; it only restores "
                "eagerly, once, at PaneGroup::new_internal/reattach_panes")
        elif name in PG_DECLINED:
            verdict, ev = "DECLINED", "TerminalView::attempt_to_share_session is a declared no-op"
        elif name in PG_UNRESOLVED:
            verdict, ev = "UNPARSED", (
                "doc's own text: 'needs a second look, not re-verified this pass' -- names the "
                "eager restore path the fork does have, may be portable rather than DIVERGENT; "
                "left unresolved by the sweep itself, not by this extraction")
        else:
            verdict, ev = "CLOUD", ("cloud/remote orchestration or the removed ambient-agent-UI "
                                     "subsystem (mod_tests.rs's own in-file header)")
        rows.append({"test": name, "pin_file": PG_FILE, "verdict": verdict, "evidence": ev,
                     "confidence": "judgement" if verdict != "UNPARSED" else "unparsed",
                     "area": "settings-workspace"})
    for r in rows:
        if r["pin_file"] == "app/src/settings_view/mod_tests.rs" and r["test"] in SW_COVERED_FIX:
            r["verdict"] = "COVERED-ELSEWHERE"
            r["evidence"] = f"renamed survivor of the dropped Code/CloudPlatform umbrella -> {SW_COVERED_FIX[r['test']]}"
            r["confidence"] = "judgement"
    return rows


# ------------------------------------------------------------------ main ---
SOURCE_DOC = {
    "app-ai": "docs/sweep/app-ai.md",
    "warp-tui": "docs/sweep/warp-tui.md",
    "app-terminal": "docs/sweep/app-terminal.md",
    "crates-ai": "docs/sweep/crates-ai.md",
    "warp-cli": "docs/sweep/warp-cli.md",
    "settings-workspace": "docs/sweep/settings-workspace.md",
}
PIN_COMMIT = "02b53fcd8"
SWEEP_DATE = "2026-08-11"
DECLARED_TOTALS = {
    "app-ai": 917, "warp-tui": 101, "app-terminal": 263,
    "crates-ai": 133, "warp-cli": 139, "settings-workspace": 288,
}


def report_reconciliation(area, area_rows):
    """Recomputed straight from the FINAL area_rows (after any hand-fix
    specials have been folded in), so this always reflects what actually
    landed in the ledger -- not an intermediate parse stage."""
    declared = DECLARED_TOTALS[area]
    print(f"\n{SOURCE_DOC[area]}: extracted {len(area_rows)} rows (doc's own declared total: {declared})")
    print(" ", Counter(r["verdict"] for r in area_rows))
    got_by_file = Counter(r["pin_file"] for r in area_rows)
    for f, n in got_by_file.items():
        want = INV.get(f, {}).get("absent")
        if want is not None and n != want:
            print(f"  per-file count differs from registry: {f} -> got {n}, registry says {want}")


def main():
    global INV
    INV = parse_inventory(read("docs/SWEEP-INVENTORY.md"))
    print(f"registry (docs/SWEEP-INVENTORY.md): {len(INV)} pin files, "
          f"{sum(v['absent'] for v in INV.values())} absent tests")

    app_ai_rows = parse_app_ai(read("docs/sweep/app-ai.md"))
    print(f"\ndocs/sweep/app-ai.md: extracted {len(app_ai_rows)} rows "
          f"(declared total {DECLARED_TOTALS['app-ai']})")
    print(" ", Counter(r["verdict"] for r in app_ai_rows))

    tui_rows, _ = parse_warp_tui(read("docs/sweep/warp-tui.md"), INV)
    report_reconciliation("warp-tui", tui_rows)

    term_rows, _ = parse_app_terminal(read("docs/sweep/app-terminal.md"), INV)
    report_reconciliation("app-terminal", term_rows)

    ca_rows, _ = parse_prose_doc(read("docs/sweep/crates-ai.md"), "crates-ai", INV,
                                  r'\n## Per-file verdicts\n(.*?)\n---')
    ca_by_file = defaultdict(dict)
    for r in ca_rows:
        if r["verdict"] != "UNPARSED":
            ca_by_file[r["pin_file"]][r["test"]] = (r["verdict"], r["evidence"], r["confidence"])
    ca_by_file = resolve_crates_ai_specials(ca_by_file, INV)
    ca_rows = [
        {"test": test, "pin_file": fpath, "verdict": v, "evidence": ev, "confidence": conf, "area": "crates-ai"}
        for fpath, tests in ca_by_file.items() for test, (v, ev, conf) in tests.items()
    ]
    report_reconciliation("crates-ai", ca_rows)

    wc_rows, _ = parse_prose_doc(read("docs/sweep/warp-cli.md"), "warp-cli", INV,
                                  r'\n## Per-file verdicts\n(.*?)\n---')
    report_reconciliation("warp-cli", wc_rows)

    sw_rows, _ = parse_prose_doc(read("docs/sweep/settings-workspace.md"), "settings-workspace",
                                  INV, r'\n## Per-file verdicts\n(.*?)\n---')
    sw_rows = apply_settings_workspace_specials(sw_rows)
    report_reconciliation("settings-workspace", sw_rows)
    for r in sw_rows:
        if r["verdict"] == "UNPARSED":
            print("  UNPARSED (left unresolved by the sweep itself):", r["pin_file"], r["test"])

    all_rows = app_ai_rows + tui_rows + term_rows + ca_rows + wc_rows + sw_rows
    declared_sum = sum(DECLARED_TOTALS.values())
    print(f"\n=== TOTAL: {len(all_rows)} rows across all six areas "
          f"(sum of each doc's own declared total: {declared_sum}) ===")
    print(" ", Counter(r["verdict"] for r in all_rows))
    print(" ", Counter(r["confidence"] for r in all_rows))

    dupc = Counter((r["pin_file"], r["test"]) for r in all_rows)
    xdups = [k for k, v in dupc.items() if v > 1]
    print(f"cross-area duplicate (pin_file,test) pairs: {len(xdups)}")
    for k in xdups[:10]:
        print("  ", k)

    def declined_ref_for(row):
        if row["verdict"] != "DECLINED":
            return ""
        m = re.search(r'#(\d+)', row["evidence"])
        return f"#{m.group(1)}" if m else ""

    missing_refs = sum(1 for r in all_rows if r["verdict"] == "DECLINED" and not declined_ref_for(r))
    declared_total = sum(1 for r in all_rows if r["verdict"] == "DECLINED")
    print(f"DECLINED rows without an extracted issue-ref: {missing_refs} of {declared_total}")

    out_path = os.path.join(REPO_ROOT, "docs", "sweep-verdict-ledger.tsv")
    cols = ["test", "pin_file", "area", "verdict", "evidence", "declined_ref",
            "pin_commit", "sweep_date", "confidence", "source_doc"]
    all_rows.sort(key=lambda r: (r["area"], r["pin_file"], r["test"]))
    with open(out_path, "w", encoding="utf-8") as f:
        f.write("\t".join(cols) + "\n")
        for r in all_rows:
            evidence = re.sub(r'\s+', ' ', r["evidence"]).strip().replace("\t", " ")
            vals = [
                r["test"], r["pin_file"], r["area"], r["verdict"], evidence,
                declined_ref_for(r), PIN_COMMIT, SWEEP_DATE, r["confidence"],
                SOURCE_DOC[r["area"]],
            ]
            f.write("\t".join(vals) + "\n")
    print(f"\nwrote {out_path} ({len(all_rows)} rows)")


if __name__ == "__main__":
    main()
