#!/usr/bin/env python3
"""Refresh the repo copy of the models.dev catalog (one command).

    python3 -I script/update_models_dev.py [--source URL_OR_FILE]

Downloads https://models.dev/api.json (or --source), validates the catalog shape,
then rewrites, deterministically:
  assets/models_dev/api.json              the uncompressed repo copy the app fetches
  assets/models_dev/README.md             provenance (source, date, sha256)
  app/resources/models_dev/api.json.gz    the bundled built-in fallback (gzip -9)
  app/resources/models_dev/PROVENANCE.txt read by the app (date) and by its unit test (sha256)
Commit all four, and merge to main so the default `agents.warp_agent.models_dev_url`
(the raw URL of assets/models_dev/api.json on main) serves the new copy.
"""
import argparse, datetime, gzip, hashlib, json, pathlib, sys, urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
DEFAULT_SOURCE = "https://models.dev/api.json"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--source", default=DEFAULT_SOURCE)
    a = ap.parse_args()
    if a.source.startswith(("http://", "https://")):
        req = urllib.request.Request(a.source, headers={"User-Agent": "phosphor-update-models-dev"})
        data = urllib.request.urlopen(req, timeout=60).read()
    else:
        data = pathlib.Path(a.source).read_bytes()
    cat = json.loads(data)
    if not isinstance(cat, dict) or len(cat) < 100:
        sys.exit("refusing: not a catalog (expected an object with >= 100 providers)")
    for pid, p in cat.items():
        if not isinstance(p, dict) or not isinstance(p.get("models", {}), dict):
            sys.exit(f"refusing: provider {pid!r} has the wrong shape")
    sha = hashlib.sha256(data).hexdigest()
    now = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    (ROOT / "assets/models_dev/api.json").write_bytes(data)
    with open(ROOT / "app/resources/models_dev/api.json.gz", "wb") as f:
        with gzip.GzipFile(fileobj=f, mode="wb", compresslevel=9, mtime=0, filename="") as g:
            g.write(data)
    (ROOT / "app/resources/models_dev/PROVENANCE.txt").write_text(
        f"source={a.source}\nfetched_at={now}\nsha256={sha}\nproviders={len(cat)}\n"
    )
    (ROOT / "assets/models_dev/README.md").write_text(
        f"""# models.dev catalog (repo copy)

`api.json` is a verbatim copy of {a.source}, the model catalog Phosphor's provider
"Quick add" row and attachment capabilities read.

- source: {a.source}
- fetched at: {now}
- providers: {len(cat)}
- sha256 of `api.json`: `{sha}`

Phosphor fetches this file from main by default (`agents.warp_agent.models_dev_url`), and
embeds a gzip of it (`app/resources/models_dev/api.json.gz`) as the built-in fallback.
Refresh both with `python3 -I script/update_models_dev.py`.

Data (c) the models.dev contributors, MIT licence: see `LICENSE-models-dev.txt`.
"""
    )
    print(f"updated: {len(cat)} providers, sha256 {sha}")


main()
