Use this template when a new feature ships behind a flag, so it is tracked, documented and reachable.

## Coding checklist
- [ ] Tested in a dev build for a while
- [ ] No sensitive information in logs
- [ ] No crashes or performance regression attributable to the feature
- [ ] Works over SSH, with no regression
- [ ] Tested on both dark and light themes
- [ ] Reachable: command palette entry and toggle setting(s), menu entry, keybinding (each where applicable)
- [ ] Keybinding does not clash with existing ones
- [ ] Accessibility (where applicable)

## Content checklist
- [ ] Help content / manual section under `docs/manual/`
- [ ] Changelog entry (below)

## Changelog

CHANGELOG-NEW-FEATURE: {{Insert a changelog entry here}}
