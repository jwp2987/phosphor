# appci/fakemcp.py (#1834): a tiny MCP server over stdio (newline-delimited JSON-RPC 2.0, stdlib only) that misbehaves on purpose, for
# cases_agent_mcp.py. Run as: python3 fakemcp.py <mode> [state-file]. It offers ONE tool, `ping` (argument `text`), and answers it by mode:
#   ok           "PONG:<text>"
#   crash_start  exits (status 3) before reading anything: the app never gets an `initialize` answer
#   crash_call   answers initialize / tools/list, then exits (status 4) in the middle of the first `ping` call, with no reply
#   hang         answers initialize / tools/list, then never answers `ping` (sleeps forever; the app's tool timeout must end the call)
#   garbage      answers initialize / tools/list, then answers `ping` with malformed JSON-RPC (a broken line, a reply with a foreign id, a non-object),
#                and a valid reply last, so a client that survives the noise still completes
#   big          answers `ping` with a 5 MB text result
#   flaky        answers the first `ping` of a launch counter 1 ("LAUNCH-1:...") and then exits (status 0); the launch counter lives in the state file, so
#                a restart answers "LAUNCH-2:..." (and so on)
# The state file also gets one line per event ("start", "call <mode>", ...) so a case can prove the server really ran and how often. handle() is pure.
import json, os, sys, time

PROTOCOL = "2024-11-05"
BIG_BYTES = 5 * 1024 * 1024
TOOL = {"name": "ping", "description": "Echo the text back (fake MCP server for the appci fault cases)",
        "inputSchema": {"type": "object", "properties": {"text": {"type": "string"}}}}


def result_text(mode, args, launch=1):
    text = str((args or {}).get("text", ""))
    if mode == "big":
        return "BIG:" + "x" * (BIG_BYTES - 4)
    if mode == "flaky":
        return "LAUNCH-%d:%s" % (launch, text)
    return "PONG:%s" % text


def handle(msg, mode, launch=1):
    """pure: one parsed JSON-RPC message -> (list of lines to write, action) where action is None | 'exit:<code>' | 'hang'. Notifications get no reply."""
    method, mid = msg.get("method"), msg.get("id")
    if mid is None:
        return [], None
    if method == "initialize":
        ver = (msg.get("params") or {}).get("protocolVersion") or PROTOCOL
        return [json.dumps({"jsonrpc": "2.0", "id": mid, "result": {"protocolVersion": ver, "capabilities": {"tools": {}},
                                                                     "serverInfo": {"name": "appci-fakemcp-" + mode, "version": "0.0.1"}}})], None
    if method == "tools/list":
        return [json.dumps({"jsonrpc": "2.0", "id": mid, "result": {"tools": [TOOL]}})], None
    if method == "ping":
        return [json.dumps({"jsonrpc": "2.0", "id": mid, "result": {}})], None
    if method == "tools/call":
        args = (msg.get("params") or {}).get("arguments") or {}
        if mode == "crash_call":
            return [], "exit:4"
        if mode == "hang":
            return [], "hang"
        if mode == "garbage":
            ok = json.dumps({"jsonrpc": "2.0", "id": mid, "result": {"content": [{"type": "text", "text": "PONG:" + str(args.get("text", ""))}]}})
            return ['{"jsonrpc": "2.0", "id": %s, "result": {"content": [{"type": "te' % json.dumps(mid), json.dumps({"jsonrpc": "2.0", "id": 987654, "result": {}}),
                    "[1, 2, 3]", "not json at all", ok], None
        res = {"content": [{"type": "text", "text": result_text(mode, args, launch)}]}
        return [json.dumps({"jsonrpc": "2.0", "id": mid, "result": res})], ("exit:0" if mode == "flaky" else None)
    return [json.dumps({"jsonrpc": "2.0", "id": mid, "error": {"code": -32601, "message": "method not found: %s" % method}})], None


def note(state, line):
    if state:
        with open(state, "a") as f:
            f.write(line + "\n")


def launches(state):
    """number of `start` lines in the state file (this launch included once main() wrote it)"""
    try:
        return sum(1 for l in open(state) if l.strip() == "start")
    except OSError:
        return 0


def main(argv):
    mode = argv[1] if len(argv) > 1 else "ok"
    state = argv[2] if len(argv) > 2 else None
    note(state, "start")
    if mode == "crash_start":
        sys.exit(3)
    launch = launches(state) if state else 1
    for raw in sys.stdin:
        raw = raw.strip()
        if not raw:
            continue
        try:
            msg = json.loads(raw)
        except ValueError:
            continue
        if msg.get("method") == "tools/call":
            note(state, "call %s" % mode)
        lines, action = handle(msg, mode, launch)
        for l in lines:
            sys.stdout.write(l + "\n")
        sys.stdout.flush()
        if action == "hang":
            while True:
                time.sleep(3600)
        if action and action.startswith("exit:"):
            os._exit(int(action[5:]))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
