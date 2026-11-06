# models.dev catalog (repo copy)

`api.json` is a verbatim copy of https://models.dev/api.json, the model catalog Phosphor's provider
"Quick add" row and attachment capabilities read.

- source: https://models.dev/api.json
- fetched at: 2026-11-06T02:22:25Z
- providers: 235
- sha256 of `api.json`: `7e6d7831b7ef34cba7b20d7a720ba647fe1069424c7ad0477c207aaede88d5a7`

Phosphor fetches this file from main by default (`agents.warp_agent.models_dev_url`), and
embeds a gzip of it (`app/resources/models_dev/api.json.gz`) as the built-in fallback.
Refresh both with `python3 -I script/update_models_dev.py`.

Data (c) the models.dev contributors, MIT licence: see `LICENSE-models-dev.txt`.
