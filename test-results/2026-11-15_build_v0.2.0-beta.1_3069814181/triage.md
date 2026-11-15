# v0.2.0-beta.1 gate run (pipeline 3069814181, 68dbe1e6c)

Beta gate (owner 2026-11-14): guards, check-linux, licenses, advisories, the 9 unit suites (Linux/macOS/Windows) and the 3 builds, all green.
App tests: mode_proof only per platform (so the driver plays the builds), all green. No other cases were played; this is not a candidate run.
The WOULD LOOSEN band lines in the driver log come from the band update over this tiny run; no band was changed (no owner OK given).
Preceding attempt 3069778549 (8f06091d7) failed in guards: version_tuple could not read 0.2.0-beta.1 and a function-local `re` import in
selftest_r32_2155.py; both fixed in 68dbe1e6c.
