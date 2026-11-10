#!/usr/bin/env python3
"""Prove that the agent fuzzer's corpus and oracle catch #1796 / #1797 / #1799 (#1836).

The corpus holds four Wave 0 scenarios (`app/src/ai/blocklist/agent_fuzz/corpus/wave0_*.json`)
that pass today because the three bugs are fixed. A test that has only ever passed proves
nothing: this runs them against the bugs.

HOW. In a separate git worktree at the commit under test, each ARM puts one bug back with a
few-line edit at the switch its fix added, builds the `warp` lib tests and runs only the corpus
replay and the I5 prompt tests. The CONTROL arm (no edit) must pass. Every other arm must make
its own scenario or test FAIL:

  arm           bug put back                                         must fail
  control       none                                                 nothing
  m1799         a reject no longer cancels the rest of the turn,     corpus: wave0_1799_reject_then_transfer
                and a hand-over with no running command waits
                for an approval no view can show
  m1797_empty   an ask_user_question with no question waits for      corpus: wave0_1797_ask_empty_args
                an answer instead of returning an error result
  m1797_valid   the answer staged by the question card is dropped    corpus: wave0_1797_ask_valid
                by `execute`, which then waits on the old channel
  m1796         a prompt-shaped line that stays unchanged is         test: agent_fuzz::prompt_tests::
                handed to the user on silence, whatever the OS       a_prompt_lookalike_is_never_handed_over_
                says about a tty read                                while_the_foreground_is_not_reading

(`wave0_1796_silent_then_handoff` is not in the table: the L1 rig has no pty, so the detector
half of #1796 is `prompt_tests.rs`. docs/testing.md "Corpus, known failures, coverage".)

WHY NOT THE PRE-FIX COMMIT. #1836 asked for a replay on `ffdaa4536` with the harness applied.
That was tried here by git alone (2026-11-10): the seven harness commits cherry-pick onto
`ffdaa4536` with conflicts only in docs/testing.md, but that harness was never compiled as it
stood; it took six later fix-up commits spread over unrelated files, and their harness hunks no
longer apply (`rig.rs` conflicts). It would also test the oracle of 2026-11-08, not the one in
use. Putting the bug back into today's tree tests today's corpus and oracle, compiles (the edits
only disable a branch), and can be repeated at any later commit. docs/testing.md "Acceptance:
the corpus catches the Wave 0 bugs" has both procedures.

USAGE
  script/ci/agent_fuzz_catches_wave0.py --check
      No cargo, no worktree. Applies every edit in memory to this checkout and fails if one of
      them no longer matches the source exactly once (the code moved: update MUTATIONS).

  script/ci/agent_fuzz_catches_wave0.py --run --workdir DIR [--commit REV] [--arms a,b,..]
                                        [--label NAME] [--keep]
      On the build box only (it runs cargo through script/agent-cargo). Creates the worktree
      DIR/wt at REV (default HEAD), runs the arms one after the other (one incremental build
      each), writes DIR/logs/<arm>.log and DIR/verdict.txt, prints the verdict table.
      DIR must be on real storage, never under /tmp. --label is the agent-cargo name (its
      target directory); the default `agent-fuzz-proof` starts cold, a label whose target is
      warm for this commit saves the first build. --keep leaves the worktree in place.

  script/ci/agent_fuzz_catches_wave0.py --cleanup --workdir DIR
      Removes the worktree DIR/wt (logs and the verdict stay).

EXIT  0: control passed and every arm run failed as expected ("CAUGHT").
      1: an arm's bug was NOT caught, the control failed, or a build failed (see the table).
      2: usage, a missing tool, or an edit that no longer applies.
"""
import argparse
import os
import re
import subprocess
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))

CORPUS_TEST = "corpus_replays_as_listed_and_reaches_the_wave0_cells"
PROMPT_TEST = "a_prompt_lookalike_is_never_handed_over_while_the_foreground_is_not_reading"
# Only what the proof needs: the corpus replay (allowlist off) and the I5 prompt tests.
NEXTEST_FILTER = f"test({CORPUS_TEST}) | test(agent_fuzz::prompt_tests)"

ACTION_MODEL = "app/src/ai/blocklist/action_model.rs"
SHELL_COMMAND = "app/src/ai/blocklist/action_model/execute/shell_command.rs"
ASK = "app/src/ai/blocklist/action_model/execute/ask_user_question.rs"
INTERACTION_MODE = "app/src/terminal/model/block/interaction_mode.rs"

# arm -> list of (file, old, new). `old` must occur EXACTLY once in the file. Each edit only
# disables the branch a fix added (or, for #1796, treats silence as a read), so the tree still
# compiles; nothing else changes.
MUTATIONS = {
    "m1799": [
        (
            ACTION_MODEL,
            "let rest_of_turn = if rejection_cancels_rest_of_turn(reason) {",
            "let rest_of_turn = if false && rejection_cancels_rest_of_turn(reason) {",
        ),
        (
            SHELL_COMMAND,
            "fn transfer_resolves_without_approval(active_block_is_long_running: bool) -> bool {\n"
            "    !active_block_is_long_running\n",
            "fn transfer_resolves_without_approval(active_block_is_long_running: bool) -> bool {\n"
            "    false && !active_block_is_long_running\n",
        ),
    ],
    "m1797_empty": [
        (
            ASK,
            "        if Self::has_no_answerable_questions(&input.action.action) {\n"
            "            return true;\n",
            "        if false && Self::has_no_answerable_questions(&input.action.action) {\n"
            "            return true;\n",
        ),
        (
            ASK,
            "        if questions.is_empty() {\n            log::warn!(\n",
            "        if false && questions.is_empty() {\n            log::warn!(\n",
        ),
    ],
    "m1797_valid": [
        (
            ASK,
            "if let Some(answers) = self.staged_answers.remove(&input.action.id) {",
            "if let Some(answers) = self.staged_answers.remove(&input.action.id).filter(|_| false) {",
        ),
    ],
    "m1796": [
        (
            INTERACTION_MODE,
            "        match foreground_tty_read() {\n",
            "        let read_or_silence = if quiet_for >= notice_after {\n"
            "            ForegroundTtyRead::Reading\n"
            "        } else {\n"
            "            foreground_tty_read()\n"
            "        };\n"
            "        match read_or_silence {\n",
        ),
    ],
}

# arm -> ("corpus", scenario) or ("test", test name suffix): what must fail in that arm.
EXPECTED = {
    "m1799": ("corpus", "wave0_1799_reject_then_transfer"),
    "m1797_empty": ("corpus", "wave0_1797_ask_empty_args"),
    "m1797_valid": ("corpus", "wave0_1797_ask_valid"),
    "m1796": ("test", PROMPT_TEST),
}
ARMS = ["control"] + list(MUTATIONS)

ANSI = re.compile(r"\x1b\[[0-9;?]*[A-Za-z]")
# A line of the corpus replay's failure message: `<scenario>: agent fuzz invariant broken: I1: ..`
SCENARIO_FAILURE = re.compile(
    r"^\s*(?:\S.*?\s)?([a-z0-9_]+)(?: \(allowlist on\))?: agent fuzz invariant broken: (\w+): (.*)$",
    re.M,
)
# nextest status lines: `        FAIL [   1.2s] warp ai::..::name`, also TIMEOUT / SIGTERM / PASS.
STATUS_LINE = re.compile(
    r"^\s*(PASS|FAIL|TIMEOUT|SIGTERM|SIGKILL|SIGABRT|LEAK)\s+\[[^\]]*\]\s+\S+\s+(\S+)", re.M
)


def mutate(text, old, new, where):
    count = text.count(old)
    if count != 1:
        raise SystemExit(
            f"agent_fuzz_catches_wave0: {where}: the text to edit occurs {count} times, expected "
            f"exactly 1. The code moved: update MUTATIONS in this script.\n--- looked for ---\n{old}"
        )
    return text.replace(old, new)


def mutated_files(tree, arm):
    """{relative path: new content} for `arm` applied to the tree at `tree`."""
    out = {}
    for rel, old, new in MUTATIONS.get(arm, []):
        if rel not in out:
            with open(os.path.join(tree, rel), encoding="utf-8") as fh:
                out[rel] = fh.read()
        out[rel] = mutate(out[rel], old, new, f"{arm}: {rel}")
    return out


def check(tree):
    for arm in MUTATIONS:
        files = mutated_files(tree, arm)
        print(f"  {arm}: {len(MUTATIONS[arm])} edit(s) apply ({', '.join(sorted(files))})")
    for arm, (kind, name) in EXPECTED.items():
        if kind == "corpus":
            path = os.path.join(tree, "app/src/ai/blocklist/agent_fuzz/corpus", name + ".json")
            if not os.path.isfile(path):
                raise SystemExit(f"agent_fuzz_catches_wave0: {arm}: corpus scenario {name}.json is gone")
        else:
            path = os.path.join(tree, "app/src/ai/blocklist/agent_fuzz/prompt_tests.rs")
            with open(path, encoding="utf-8") as fh:
                if f"fn {name}(" not in fh.read():
                    raise SystemExit(
                        f"agent_fuzz_catches_wave0: {arm}: test {name} is gone from prompt_tests.rs"
                    )
    print("agent_fuzz_catches_wave0: every edit applies exactly once; every expected scenario / test exists")


def git(args, cwd, check_rc=True):
    # rerere off: this must not record or replay conflict resolutions in the shared repository.
    proc = subprocess.run(
        ["git", "-c", "rerere.enabled=false"] + args, cwd=cwd, text=True, capture_output=True
    )
    if check_rc and proc.returncode != 0:
        raise SystemExit(f"git {' '.join(args)} failed in {cwd}:\n{proc.stdout}{proc.stderr}")
    return proc


def parse_log(text):
    """(statuses {test name: status}, scenario failures {scenario: (invariant, detail)})."""
    text = ANSI.sub("", text)
    statuses = {}
    for m in STATUS_LINE.finditer(text):
        # The last line wins (nextest prints a failing test again in its final summary).
        statuses[m.group(2)] = m.group(1)
    scenarios = {}
    for m in SCENARIO_FAILURE.finditer(text):
        scenarios.setdefault(m.group(1), (m.group(2), m.group(3).strip()[:160]))
    return statuses, scenarios


def judge(arm, rc, statuses, scenarios):
    """(verdict, detail). Verdicts: OK / FAILED (control); CAUGHT / NOT CAUGHT / INCONCLUSIVE /
    BUILD FAILED (the other arms)."""
    corpus_status = next((s for name, s in statuses.items() if name.endswith(CORPUS_TEST)), None)
    if not statuses:
        return "BUILD FAILED", f"no test ran (cargo exit {rc}): read the log"
    if arm == "control":
        bad = {name: s for name, s in statuses.items() if s != "PASS"}
        if bad or rc != 0:
            return "FAILED", "the unedited tree must pass: " + ", ".join(
                f"{n} {s}" for n, s in sorted(bad.items())
            )
        return "OK", f"{len(statuses)} tests passed with no edit"
    kind, name = EXPECTED[arm]
    if kind == "corpus":
        if name in scenarios:
            invariant, detail = scenarios[name]
            others = sorted(s for s in scenarios if s != name)
            also = f"; also failed: {', '.join(others)}" if others else ""
            return "CAUGHT", f"{name} fails {invariant} ({detail}){also}"
        if corpus_status in ("TIMEOUT", "SIGTERM", "SIGKILL", "SIGABRT"):
            return "INCONCLUSIVE", f"the corpus replay ended {corpus_status} with no line for {name}"
        if corpus_status is None:
            return "INCONCLUSIVE", "the corpus replay did not run"
        return "NOT CAUGHT", f"{name} passed with the bug put back (corpus replay: {corpus_status})"
    status = next((s for test, s in statuses.items() if test.endswith(name)), None)
    if status == "FAIL":
        also = sorted(
            t.rsplit("::", 1)[-1] for t, s in statuses.items() if s == "FAIL" and not t.endswith(name)
        )
        return "CAUGHT", f"{name} fails" + (f"; also failed: {', '.join(also)}" if also else "")
    if status is None:
        return "INCONCLUSIVE", f"{name} did not run"
    if status == "PASS":
        return "NOT CAUGHT", f"{name} passed with the bug put back"
    return "INCONCLUSIVE", f"{name} ended {status}"


def run(args):
    workdir = os.path.abspath(args.workdir)
    if workdir == "/tmp" or workdir.startswith("/tmp/"):
        raise SystemExit("agent_fuzz_catches_wave0: --workdir must be on real storage, not under /tmp")
    arms = args.arms.split(",") if args.arms else ARMS
    unknown = [a for a in arms if a not in ARMS]
    if unknown:
        raise SystemExit(f"agent_fuzz_catches_wave0: unknown arm(s) {unknown}; arms are {ARMS}")
    if "control" not in arms:
        print("note: no control arm; a CAUGHT without a passing control proves nothing", file=sys.stderr)
    wt = os.path.join(workdir, "wt")
    logs = os.path.join(workdir, "logs")
    os.makedirs(logs, exist_ok=True)
    commit = git(["rev-parse", "--verify", f"{args.commit}^{{commit}}"], ROOT).stdout.strip()
    if os.path.isdir(wt):
        git(["checkout", "-q", "--", "."], wt)
        git(["checkout", "-q", "--detach", commit], wt)
    else:
        git(["worktree", "add", "-q", "--detach", wt, commit], ROOT)
    # The edits must apply at this commit before anything is built.
    check(wt)
    agent_cargo = os.path.join(wt, "script", "agent-cargo")
    if not os.access(agent_cargo, os.X_OK):
        raise SystemExit("agent_fuzz_catches_wave0: script/agent-cargo is missing in the worktree")
    env = dict(os.environ)
    # What script/agent_fuzz sets: a stalled provider must end in test time.
    env.setdefault("PHOSPHOR_BYOP_STREAM_IDLE_TIMEOUT_SECS", "1")
    env.setdefault("PHOSPHOR_BYOP_STREAM_FIRST_EVENT_TIMEOUT_SECS", "1")
    results = []
    for arm in arms:
        git(["checkout", "-q", "--", "."], wt)
        for rel, content in mutated_files(wt, arm).items():
            with open(os.path.join(wt, rel), "w", encoding="utf-8") as fh:
                fh.write(content)
        diff = git(["diff", "--stat"], wt).stdout.strip() or "(no edit)"
        log_path = os.path.join(logs, f"{arm}.log")
        cmd = [
            agent_cargo, args.label, "nextest", "run",
            # The package and feature set of script/agent_fuzz, so a target directory that is
            # warm from it is reused as it is.
            "-p", "fake_llm", "-p", "warp", "--lib", "--features", "warp/gui",
            "-E", NEXTEST_FILTER, "--no-fail-fast", "--failure-output", "immediate-final",
        ]  # fmt: skip
        print(f"\n=== arm {arm} at {commit[:9]}\n{diff}\n$ {' '.join(cmd)}\n(log: {log_path})", flush=True)
        with open(log_path, "w", encoding="utf-8") as log:
            log.write(f"# arm {arm} commit {commit}\n# {' '.join(cmd)}\n# edit:\n{git(['diff'], wt).stdout}\n")
            log.flush()
            proc = subprocess.run(cmd, cwd=wt, env=env, stdout=log, stderr=subprocess.STDOUT)
        with open(log_path, encoding="utf-8", errors="replace") as fh:
            statuses, scenarios = parse_log(fh.read())
        verdict, detail = judge(arm, proc.returncode, statuses, scenarios)
        results.append((arm, verdict, detail))
        print(f"    {arm}: {verdict}: {detail}", flush=True)
    git(["checkout", "-q", "--", "."], wt)
    table = [f"agent fuzzer: does the corpus catch the Wave 0 bugs? commit {commit}"]
    table += [f"  {arm:<12} {verdict:<13} {detail}" for arm, verdict, detail in results]
    proven = "control" in arms and all(
        verdict == ("OK" if arm == "control" else "CAUGHT") for arm, verdict, _ in results
    )
    table.append(
        "RESULT: PROVEN for the arms run"
        if proven
        else "RESULT: NOT PROVEN (a NOT CAUGHT is a gap in the corpus or the oracle: file it)"
    )
    text = "\n".join(table)
    with open(os.path.join(workdir, "verdict.txt"), "w", encoding="utf-8") as fh:
        fh.write(text + "\n")
    print("\n" + text)
    if not args.keep:
        git(["worktree", "remove", "--force", wt], ROOT, check_rc=False)
    return 0 if proven else 1


def self_test():
    """The log parser and the judge, on fixtures (no cargo, no git)."""
    corpus = f"ai::blocklist::agent_fuzz::corpus_replay_tests::{CORPUS_TEST}"
    prompt = f"ai::blocklist::agent_fuzz::prompt_tests::{PROMPT_TEST}"
    log_fail = (
        f"        FAIL [  41.2s] warp {corpus}\n"
        "--- STDERR ---\n"
        "thread 'x' panicked at app/src/ai/blocklist/agent_fuzz/corpus_replay_tests.rs:160:5:\n"
        "wave0_1799_reject_then_transfer: agent fuzz invariant broken: I2a: a pending action has no card\n"
        'scenario (save as ..):\n{ "v": 1 }\n\n'
        "hang1851_ask_empty_args: agent fuzz invariant broken: I1: the turn never ended\n"
        f"        PASS [   0.1s] warp {prompt}\n"
    )
    log_pass = f"        PASS [  39.0s] warp {corpus}\n        PASS [   0.1s] warp {prompt}\n"
    log_prompt_fail = f"        PASS [  39.0s] warp {corpus}\n        FAIL [   0.1s] warp {prompt}\n"
    log_timeout = f"     TIMEOUT [ 600.0s] warp {corpus}\n"
    cases = [
        ("control passes", "control", 0, log_pass, "OK"),
        ("control with a failure", "control", 100, log_fail, "FAILED"),
        ("the scenario fails", "m1799", 100, log_fail, "CAUGHT"),
        ("another scenario fails, not ours", "m1797_valid", 100, log_fail, "NOT CAUGHT"),
        ("everything passes with the bug back", "m1797_empty", 0, log_pass, "NOT CAUGHT"),
        ("the prompt test fails", "m1796", 100, log_prompt_fail, "CAUGHT"),
        ("the prompt test passes", "m1796", 0, log_pass, "NOT CAUGHT"),
        ("a timeout is no verdict", "m1799", 100, log_timeout, "INCONCLUSIVE"),
        ("nothing ran", "m1799", 101, "error: could not compile `warp`\n", "BUILD FAILED"),
    ]
    failed = 0
    for name, arm, rc, log, want in cases:
        statuses, scenarios = parse_log(log)
        got, detail = judge(arm, rc, statuses, scenarios)
        if got != want:
            failed += 1
            print(f"FAIL self-test: {name}: expected {want}, got {got} ({detail})")
    _, scenarios = parse_log(log_fail)
    if scenarios.get("wave0_1799_reject_then_transfer", ("", ""))[0] != "I2a":
        failed += 1
        print(f"FAIL self-test: the invariant of a scenario line: {scenarios}")
    if failed:
        return 1
    print(f"agent_fuzz_catches_wave0: self-test ok ({len(cases) + 1} checks)")
    return 0


def main():
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--check", action="store_true")
    mode.add_argument("--self-test", action="store_true")
    mode.add_argument("--run", action="store_true")
    mode.add_argument("--cleanup", action="store_true")
    parser.add_argument("--workdir")
    parser.add_argument("--commit", default="HEAD")
    parser.add_argument("--arms", help="comma-separated, from: " + ",".join(ARMS))
    parser.add_argument("--label", default="agent-fuzz-proof")
    parser.add_argument("--keep", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    if args.check:
        check(ROOT)
        return 0
    if not args.workdir:
        parser.error("--workdir DIR is required (real storage, not /tmp)")
    if args.cleanup:
        git(["worktree", "remove", "--force", os.path.join(os.path.abspath(args.workdir), "wt")], ROOT)
        return 0
    return run(args)


if __name__ == "__main__":
    sys.exit(main())
