# v0.21 shield, library sidebar, vertical tabs and new-tab tiles: design check

## Audit

`sloplint --scope app` on `src/ui/toolbar.html` (the new-tab page is part of the same
document):

| Report | FAIL | WARN | NOTE |
| --- | --- | --- | --- |
| `audit-before.txt` (`4ece8ee`, main) | 30 | 2 | 0 |
| `audit-after.txt` (this branch) | 30 | 2 | 0 |

The 30 FAILs are gate 48 on the theme blocks (`body[data-theme=...]`), waived by the
owner in PR #27. The two WARNs are the same dismissed ones: 41 (the favourite card
inherits `--text`, covered by the contrast tests) and F23 (a meta description on an
internal page). The first after-run found one new WARN (the shield panel set no text
colour) and six spacings off the 4px scale; all were fixed before the final run.

## Screenshots

- `before-*`: main at `4ece8ee`, toolbar view only (`layout_screenshots`).
- `after-toolbar-*`: this branch, toolbar view only, the same tool and pages
  (new tab, website, menu sidebar).
- `after-*` (other names): this branch, whole window, made by `window_screenshots`
  from each view's own capture placed at its layout position: `website`, `shield`
  (panel open, test blocker added, 2 requests blocked), `library` (history beside
  the page), `vertical-website`, `vertical-library` and `vertical-newtab` (tiles: an
  icon tile and a letter tile, Glow background).

Each in light and dark at 100% and 150%. 150% is emulated in each view by the engine
(`Emulation.setDeviceMetricsOverride`); no display setting was changed.
