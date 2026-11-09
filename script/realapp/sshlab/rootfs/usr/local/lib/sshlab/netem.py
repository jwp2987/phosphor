# netem.py: the lab's network-fault commands as pure data (no subprocess here: labd.py runs them, appci/selftest.py checks them).
# Faults apply ONLY to the ssh ports (22, 2222) and the echo probe (7), never to the control port 7000, inside the container's own netns.
#   egress (server -> client): root `prio` qdisc, 3 bands, every packet defaults to band 3; a u32 filter on the SOURCE port sends ssh/probe
#     packets to band 1, which carries the netem qdisc. Everything else (the control channel) stays in band 3 untouched.
#   ingress (client -> server): an `ingress` qdisc whose u32 filters on the DESTINATION port mirror packets to ifb0, which carries netem.
#   The RTT is split half and half between the two directions.
FAULT_PORTS = (22, 2222, 7)
CONTROL_PORT = 7000
IFACE, IFB = "eth0", "ifb0"
LIMITS = dict(rtt_ms=(0, 5000), jitter_ms=(0, 1000), loss_pct=(0, 100), rate_kbit=(0, 10_000_000))


def normalize(spec):
    """validate a fault spec dict {rtt_ms, jitter_ms, loss_pct, rate_kbit}; unknown keys and out-of-range values raise ValueError. Returns floats."""
    out = {}
    for k, (lo, hi) in LIMITS.items():
        v = float((spec or {}).get(k, 0) or 0)
        if not lo <= v <= hi:
            raise ValueError("%s=%s outside %s..%s" % (k, v, lo, hi))
        out[k] = v
    extra = set(spec or {}) - set(LIMITS)
    if extra:
        raise ValueError("unknown fault keys: %s" % sorted(extra))
    return out


def _num(x):
    return ("%g" % x)


def netem_args(spec, direction_share=0.5):
    """the `netem ...` argument list for one direction: half of the RTT/jitter, the whole loss and rate"""
    s = normalize(spec)
    a = ["netem"]
    d = s["rtt_ms"] * direction_share
    j = s["jitter_ms"] * direction_share
    if d or j:
        a += ["delay", _num(d) + "ms"] + ([_num(j) + "ms"] if j else [])
    if s["loss_pct"]:
        a += ["loss", _num(s["loss_pct"]) + "%"]
    if s["rate_kbit"]:
        a += ["rate", _num(s["rate_kbit"]) + "kbit"]
    return a


def is_noop(spec):
    s = normalize(spec)
    return not any(s.values())


def apply_commands(spec, ports=FAULT_PORTS, iface=IFACE, ifb=IFB):
    """argv lists that (re)install the fault; idempotent: starts by clearing. A no-op spec yields just the clear commands."""
    cmds = clear_commands(iface, ifb)
    if is_noop(spec):
        return cmds
    ne = netem_args(spec)
    cmds += [
        ["ip", "link", "add", ifb, "type", "ifb"],
        ["ip", "link", "set", ifb, "up"],
        ["tc", "qdisc", "add", "dev", iface, "root", "handle", "1:", "prio", "bands", "3", "priomap"] + ["2"] * 16,
        ["tc", "qdisc", "add", "dev", iface, "parent", "1:1", "handle", "10:"] + ne,
        ["tc", "qdisc", "add", "dev", iface, "handle", "ffff:", "ingress"],
        ["tc", "qdisc", "add", "dev", ifb, "root", "handle", "2:"] + ne,
    ]
    for p in ports:
        cmds.append(["tc", "filter", "add", "dev", iface, "parent", "1:", "protocol", "ip", "prio", "1", "u32", "match", "ip", "sport", str(p), "0xffff", "flowid", "1:1"])
        cmds.append(["tc", "filter", "add", "dev", iface, "parent", "ffff:", "protocol", "ip", "prio", "1", "u32", "match", "ip", "dport", str(p), "0xffff",
                     "action", "mirred", "egress", "redirect", "dev", ifb])
    return cmds


def clear_commands(iface=IFACE, ifb=IFB):
    """removing what apply_commands installed; each command may fail when nothing was installed (labd ignores the status of these)"""
    return [["tc", "qdisc", "del", "dev", iface, "root"], ["tc", "qdisc", "del", "dev", iface, "ingress"], ["ip", "link", "del", ifb]]


def nft_block_outbound(uids, iface=IFACE):
    """nftables ruleset (text for `nft -f -`) that refuses NEW outbound connections made by the lab users, except on loopback: ssh replies are
    established flows and unaffected, but an installer's curl/wget (or nested ssh) cannot reach the network. Replaces itself (delete-table first)."""
    ids = ", ".join(str(u) for u in sorted(uids))
    return ("table inet sshlab_out\ndelete table inet sshlab_out\n"
            "table inet sshlab_out {\n  chain out {\n    type filter hook output priority 0; policy accept;\n"
            "    meta skuid { %s } oifname != \"lo\" ct state new reject\n  }\n}\n" % ids)


def nft_drop_connection(client_ip, client_port, mode, server_port):
    """argv-free nft rule text for one connection: 'blackhole' drops both directions silently; 'rst' answers the client's next packet with a RST.
    Rules go in table inet sshlab_drop (created on demand by labd, flushed on /fault DELETE or after the timeout)."""
    if mode not in ("blackhole", "rst"):
        raise ValueError("mode must be blackhole or rst")
    sel_in = "ip saddr %s tcp sport %d tcp dport %d" % (client_ip, client_port, server_port)
    sel_out = "ip daddr %s tcp dport %d tcp sport %d" % (client_ip, client_port, server_port)
    if mode == "rst":
        return ["add rule inet sshlab_drop in %s reject with tcp reset" % sel_in]
    return ["add rule inet sshlab_drop in %s drop" % sel_in, "add rule inet sshlab_drop out %s drop" % sel_out]


NFT_DROP_SETUP = ("table inet sshlab_drop\ndelete table inet sshlab_drop\ntable inet sshlab_drop {\n"
                  "  chain in { type filter hook input priority -10; policy accept; }\n"
                  "  chain out { type filter hook output priority -10; policy accept; }\n}\n")
