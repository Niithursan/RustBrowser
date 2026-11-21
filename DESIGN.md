---
name: Rust Browser
description: Quiet, exact browser chrome for Windows that reports state honestly and leaves the page the room.
colors:
  graphite-strip: "#131518"
  graphite-surface: "#1E2228"
  graphite-panel: "#191C21"
  graphite-card: "#272C34"
  graphite-field: "#15181C"
  instrument-white: "#E9ECEF"
  dial-grey: "#9AA5B1"
  hairline: "#3A414B"
  field-hairline: "#656E79"
  disabled-grey: "#5E6772"
  warning-coral: "#FF9B8F"
  signal-amber: "#F2C94C"
  on-signal: "#1E2228"
  harbour-blue: "#7CC4E4"
  lilac: "#E3B5FF"
  cobalt: "#2446B8"
  paper-strip: "#D9DEE3"
  paper-surface: "#EEF0F2"
  paper-panel: "#F8F9FA"
  paper-card: "#E2E6EA"
  paper-field: "#FFFFFF"
  ink: "#1B2430"
  ink-secondary: "#4A5563"
  paper-hairline: "#A7B0BA"
  paper-field-hairline: "#808A95"
  paper-disabled: "#9BA4AE"
  warning-crimson: "#A3192A"
typography:
  display:
    fontFamily: "'Segoe UI Variable Display', 'Segoe UI', sans-serif"
    fontSize: "34px"
    fontWeight: 600
    lineHeight: 1.1
    letterSpacing: "-0.015em"
  headline:
    fontFamily: "'Segoe UI Variable Display', 'Segoe UI', sans-serif"
    fontSize: "28px"
    fontWeight: 600
    lineHeight: 1.15
    letterSpacing: "-0.01em"
  title:
    fontFamily: "'Segoe UI Variable Display', 'Segoe UI', sans-serif"
    fontSize: "19px"
    fontWeight: 600
    lineHeight: 1.3
  display-narrow:
    fontFamily: "'Segoe UI Variable Display', 'Segoe UI', sans-serif"
    fontSize: "26px"
    fontWeight: 600
    lineHeight: 1.1
    letterSpacing: "-0.015em"
  headline-column:
    fontFamily: "'Segoe UI Variable Display', 'Segoe UI', sans-serif"
    fontSize: "22px"
    fontWeight: 600
    lineHeight: 1.15
  subtitle:
    fontFamily: "'Segoe UI Variable Display', 'Segoe UI', sans-serif"
    fontSize: "15px"
    fontWeight: 600
    lineHeight: 1.3
  dropdown-heading:
    fontFamily: "'Segoe UI Variable Display', 'Segoe UI', sans-serif"
    fontSize: "14px"
    fontWeight: 600
    lineHeight: 1.3
  search-field:
    fontFamily: "'Segoe UI Variable Text', 'Segoe UI', sans-serif"
    fontSize: "17px"
    fontWeight: 400
  address-origin:
    fontFamily: "'Segoe UI Variable Display', 'Segoe UI', sans-serif"
    fontSize: "15.5px"
    fontWeight: 600
    letterSpacing: "-0.005em"
  address:
    fontFamily: "'Segoe UI Variable Text', 'Segoe UI', sans-serif"
    fontSize: "15px"
    fontWeight: 400
  body:
    fontFamily: "'Segoe UI Variable Text', 'Segoe UI', sans-serif"
    fontSize: "13px"
    fontWeight: 400
    lineHeight: 1.4
  label:
    fontFamily: "'Segoe UI Variable Text', 'Segoe UI', sans-serif"
    fontSize: "12px"
    fontWeight: 400
    lineHeight: 1.4
rounded:
  xs: "4px"
  sm: "6px"
  md: "8px"
  lg: "10px"
  pill-28: "14px"
  pill-30: "15px"
  pill-32: "16px"
  pill-34: "17px"
  location: "18px"
  search: "24px"
spacing:
  hair: "2px"
  xs: "4px"
  sm: "8px"
  md: "12px"
  lg: "16px"
  xl: "24px"
  xxl: "32px"
components:
  button:
    backgroundColor: "{colors.graphite-card}"
    textColor: "{colors.instrument-white}"
    rounded: "{rounded.sm}"
    padding: "0 12px"
    height: "32px"
  button-quiet:
    backgroundColor: "transparent"
    textColor: "{colors.instrument-white}"
    rounded: "{rounded.sm}"
    padding: "0 12px"
    height: "32px"
  nav-icon-button:
    backgroundColor: "transparent"
    textColor: "{colors.instrument-white}"
    rounded: "{rounded.pill-34}"
    size: "34px"
  input:
    backgroundColor: "{colors.graphite-field}"
    textColor: "{colors.instrument-white}"
    rounded: "{rounded.sm}"
    padding: "0 12px"
    height: "32px"
  location-line:
    backgroundColor: "{colors.graphite-field}"
    textColor: "{colors.instrument-white}"
    typography: "{typography.address}"
    rounded: "{rounded.location}"
    height: "36px"
  security-badge:
    backgroundColor: "rgba(233,236,239,.07)"
    textColor: "{colors.instrument-white}"
    typography: "{typography.label}"
    rounded: "{rounded.pill-28}"
    padding: "0 12px 0 8px"
    height: "28px"
  security-lock:
    backgroundColor: "transparent"
    textColor: "{colors.dial-grey}"
    rounded: "{rounded.pill-28}"
    size: "28px"
  tab:
    backgroundColor: "transparent"
    textColor: "{colors.dial-grey}"
    typography: "{typography.body}"
    rounded: "10px 10px 0 0"
    padding: "0 4px 0 12px"
    height: "36px"
  tab-active:
    backgroundColor: "{colors.graphite-surface}"
    textColor: "{colors.instrument-white}"
    rounded: "10px 10px 0 0"
    height: "36px"
  dropdown:
    backgroundColor: "{colors.graphite-panel}"
    textColor: "{colors.instrument-white}"
    rounded: "{rounded.lg}"
    padding: "8px"
  favorite-card:
    backgroundColor: "{colors.graphite-surface}"
    textColor: "{colors.instrument-white}"
    rounded: "{rounded.lg}"
    padding: "10px 12px"
  private-label:
    backgroundColor: "{colors.signal-amber}"
    textColor: "{colors.on-signal}"
    typography: "{typography.label}"
    rounded: "{rounded.xs}"
    padding: "2px 8px"
---

# Design System: Rust Browser

## Overview

**Creative North Star: "The Instrument Panel"**

Rust Browser's chrome is an instrument panel for the web: a row of precise, legible
controls whose job is to report state honestly and get out of the way. Every reading
means something. An encrypted page, the normal case, shows only a quiet lock; the
connection badge speaks up in plain words when there is something to read (*Not
encrypted*, *Not connected*, *Browser page*); the shield appears only once an extension
can block requests and shows a number only when it is counting; the loading bar moves
only while a page loads. Nothing on the panel is decoration, and nothing
pretends to know more than it does.

The panel is graphite and restrained: four stacked tonal layers (tab strip, toolbar,
panel, card) separated by hairlines rather than shadows, one typeface family from
Windows itself, and a single **signal** colour per theme that marks what is active,
focused or in progress. The address line is the one bold element; it carries the
site's origin in a heavier display cut so the user always knows where they are. All
else stays quiet so the website, not the browser, is the picture.

The panel has its own look rather than imitating Fluent or Edge, and merges the tab strip
into the top of a frameless Windows window with flush `46x36` Minimize, Maximize/Restore
and Close caption buttons at the top-right corner while preserving Windows 11 rounded
corners, theme-coloured borders, Snap, Narrator, high contrast, reduced motion and display
scaling (checked at 100% and 150%). Every edge resizes a restored window: Windows keeps
its invisible resize border outside the left, right and bottom edges (the window
subclasses `WM_NCCALCSIZE` so only the caption is removed), and a 4px strip
(`#resize-top`) along the top of the chrome resizes the top edge and its corners.
Density is compact and desktop-first: 32px controls, 13px text, 4px spacing steps.

**Key Characteristics:**
- Graphite tonal layers and 1px hairlines; shadows only on things that float.
- One signal colour per theme, used for state, never for decoration.
- The address line is the single emphasised element; the origin is set heavier than the path.
- Segoe UI Variable throughout: Display cut for headings and the origin, Text cut for everything else.
- Plain-word state readouts over icons alone; every icon button has a name and a shortcut in its tooltip.
- Compact desktop density: 32px controls, 13px body, a 4px spacing scale.

## Colors

Cool graphite neutrals with one saturated signal per theme; the Light theme inverts to
cool paper and ink with a cobalt signal.

The browser ships four themes. **Dark** (the default) defines every token; **Slate**
and **Plum** retint only the five surfaces, the hairline and the signal, inheriting
text, disabled and warning colours from Dark; **Light** redefines the full set. Theme
values live on `:root` and `body[data-theme=…]` in `src/ui/toolbar.html`, and their
names are read by the `ui::tests` contrast checks, so they must stay stable.

### Primary (the signal, one per theme)
- **Signal Amber** (Dark): selected-tab top rule, focus rings, loading bar, saved-bookmark
  star, checkbox and progress accent, the private-window label, the
  permission-request border, and the Glow new-tab background tint.
- **Harbour Blue** (Slate): the same roles in the Slate theme.
- **Lilac** (Plum): the same roles in the Plum theme.
- **Cobalt** (Light): the same roles in the Light theme, with white text on it.
- **On Signal** (Dark/Slate/Plum: graphite surface; Light: white): text set on a signal fill.

### Neutral (Dark)
- **Graphite Strip**: the tab strip and the vertical tab column; the darkest layer.
- **Graphite Surface**: the toolbar row, the selected tab (which joins it), favourite cards.
- **Graphite Panel**: dropdowns, the library column, Privacy & security and the new-tab page.
- **Graphite Card**: default button fill and favourite-tile backing.
- **Graphite Field**: the address line and text inputs; slightly darker than the surface so the field reads as a recess.
- **Instrument White**: primary text.
- **Dial Grey**: secondary text, inactive tab titles, icons at rest, hover borders.
- **Hairline**: every 1px border and divider.
- **Field Hairline** (`--field-line`): the address line's border only, a step brighter than Hairline so the field's edge keeps 3:1 against the toolbar and the field (WCAG 1.4.11). Paper Field Hairline is its Light value; Slate and Plum retint it.
- **Disabled Grey**: disabled control text.
- **Warning Coral**: *Not encrypted* / failed connection, notices, settings errors.
- **Warning Coral** also marks destructive buttons; a notice in it is always a problem.
- Hover and pressed states are translucent washes of the text colour (7% and 14% on dark, 6% and 12% on light), not separate hues. The menu backdrop over a full panel is a 45% black scrim (28% ink on Light).

### Neutral (Light)
- **Paper Strip / Paper Surface / Paper Panel / Paper Card / Paper Field**: the same five layers, from greyest to white.
- **Ink** and **Ink Secondary**: primary and secondary text.
- **Paper Hairline**, **Paper Disabled**, **Warning Crimson**: as their dark counterparts.

### Named Rules
**The One Signal Rule.** Each theme has exactly one signal colour, and it only ever means *active, focused, in progress, private or waiting for your decision*. It never fills a large area at full strength: its biggest solid uses are the 2px loading bar, the selected tab's 2px top rule and the small private label. The one area use is the new-tab **Glow** background, which mixes 10% of the signal into the surface and only when the user chooses Glow.

**The Theme-Token Rule.** New surfaces use the theme variables (`--surface`, `--panel`, `--accent`, …), never literal colours, so all four themes and high contrast work without extra code. Website colours never enter the chrome; new-tab backgrounds are mixed only from the current theme's own tokens.

## Typography

**Display Font:** Segoe UI Variable Display (with Segoe UI, sans-serif)
**Body Font:** Segoe UI Variable Text (with Segoe UI, sans-serif)

**Character:** Windows' own variable family, used in its two optical cuts: the Display
cut tightened slightly for headings and the address origin, the Text cut for every
control. It reads as part of the operating system without copying its controls, and
no web font is ever loaded (the toolbar's security policy forbids it).

### Hierarchy
- **Display** (600, 34px, 1.1, −0.015em): the new-tab question "Where would you like to go?" only. **Display narrow** (26px) replaces it under 700px.
- **Headline** (600, 28px, 1.15, −0.01em): full-panel titles. **Headline column** (22px) replaces it in the library column.
- **Title** (600, 19px, 1.3): section headings on full panels (Background tabs, Appearance, Extensions).
- **Subtitle** (600, 15px, 1.3): sub-sections (Favorites, Recent pages).
- **Dropdown heading** (600, 14px, 1.3): the heading of a dropdown (site information, star, shield).
- **Search field** (400, 17px): the new-tab search field only.
- **Address origin** (600, 15.5px, −0.005em, Display cut): the host in the address line; the rest of the URL follows at 15px in Dial Grey.
- **Body** (400, 13px, 1.4; 1.5 on full panels): every control, menu item and tab title. Library row titles step up to 14px/600.
- **Label** (400, 12px): secondary text, form labels, the connection word (600), the shield count (600), shortcut lists.

### Named Rules
**The Origin-First Rule.** The site's origin is always the largest emphasised text in the toolbar. Nothing else in the tab strip, navigation row or bookmarks bar may be set at 600 weight and 15px or larger. The selected tab's title keeps regular weight at 13px; selection is marked by colour, surface and the top signal rule, so it never competes with the origin. Headings on full panels such as the library and new tab are below the toolbar and exempt.

**The Plain Word Rule.** State is written as a word as well as shown by an icon or colour (*Not encrypted*, a blocked-request count, "Private"), so it survives high contrast, colour blindness and Narrator. The one exception is the normal case, an encrypted connection: it is a quiet lock with no word on screen (its button is still named "Encrypted" for Narrator), so the word appears only when the state is worth reading and a change of state is a change of shape.

## Layout

The chrome is a stack of rows across the top of the window: the **tab strip** (min 42px
restored with 6px top padding, 36px maximized, tabs bottom-aligned, a 1px hairline along
its bottom that the selected tab covers, then the scroll arrows when tabs overflow, New
tab (+) and the all-tabs list (a down chevron) right after the tabs, an open drag area,
and `46x36` Minimize, Maximize/Restore and Close caption buttons flush with the
top-right window edge), the
**navigation row** (4px top, 8px bottom padding; hosting the flush caption buttons at its
right end when vertical tabs are on) with back, forward, reload/stop and home on the left,
the address line filling the middle and the profile button (with more than one profile)
and Menu (⋮) at the right, an optional **bookmarks bar** (28px items with `16x16` site
icons, borderless `4px`-rounded letter monogram chips in Graphite Card at 600, or 16px
folder icons in the text colour; All bookmarks sits at the right end behind a hairline
with its own library glyph, because it opens the library column rather than a folder),
a notice row, and the 2px **loading bar** (carrying a 1px bottom hairline while
idle so the toolbar cleanly separates from full panels and light webpages). The page view
starts below `--chrome-height` (120px).

The same trusted document is also used for two side views, chosen by `data-role`: the
**vertical tab column** (one tab per row, 4px gaps, hairline on its right edge) and the
**library column** at the right of a live page or the new-tab page (260-640px wide,
hairline on its left edge, an 8px drag handle, and a fixed 36px top caption strip holding
the flush window controls whenever `New tab` sits beside it). Full panels (Settings, new tab) are padded
24px × 32px with content capped at 1040px (Settings: one centred 760px column) and explanatory prose capped at 68ch (`text-wrap: pretty`); panel headers and the Settings section bar stick to the top with an active-section signal underline so section navigation and Back to page (F7) stay reachable while scrolling. Settings sections are separated by 40px of space, not a rule (a rule doubled the section bar's hairline whenever a section was scrolled to); each explanation is one sentence, with any detail in a quiet disclosure. Its sections are Appearance, Profiles, Search, Background tabs, Privacy & security, Extensions, Clear browsing data and System (Default browser and Diagnostics); the new-tab body is centred at 860px with creation and page-customization forms tucked behind quiet disclosures.

Spacing follows a 4px scale (4, 8, 12, 16, 20, 24, 32, 56) with 2px reserved for the
tight gaps between tabs and toolbar buttons. Under 700px the navigation row wraps and
the address line takes its own full-width line, while the profile button and Menu stay
at the right edge; full panels drop to 16px padding.

## Elevation & Depth

Flat by default. Depth comes from tonal layering: the strip is the darkest layer, the
toolbar sits one step lighter, the selected tab takes the toolbar's colour so it reads
as joined to it, and fields sit a step darker as recesses. Edges are 1px hairlines.

### Shadow Vocabulary
- **Float** (`box-shadow: 0 6px 18px rgba(0,0,0,.35)`; Light: `rgba(27,36,48,.18)`): dropdowns, the address context menu and the confirmation dialog, the only floating surfaces.
- **Signal rule** (a 2px bar along the selected tab's top edge, inset 10px from each side so it never bends around the rounded corners): an inset rule, not an elevation. In the vertical column the selection is a short 3px rounded bar instead (see Tabs).
- **Focus ring** (`outline: 2px solid <signal>; outline-offset: 2px`): every focusable control. The address line instead shifts its border to the signal and adds a 1px signal ring. A tab instead rings its whole card inset 5px (`outline-offset: -5px`), so the ring stays 3px clear of the signal rule along the top and is never clipped by the strip.

### Named Rules
**The Float-Only Rule.** Only a surface that floats over the page gets a shadow. Cards, buttons, tabs and panels never do.

**The Live-Page Rule.** While a dropdown is open, the page behind it stays live and undimmed: video keeps playing and a press on the page closes the dropdown. Over a webpage the toolbar is clipped to the chrome and the dropdown's rounded shape, so the dropdown's own shadow is not drawn there; its Hairline border marks its edge. Over a full panel (the new-tab page, Settings) the panel is dimmed by the menu backdrop scrim.

## Shapes

Gently rounded, with the radius following function. Rectangular controls (buttons,
inputs, menu items) use 6px; dropdowns, favourite cards and tab tops use 10px; the
favourite tile uses 8px; small badges use 4px. Things the eye scans along (the address
line, the security badge, the shield, the round navigation buttons and the new-tab
search) are fully rounded at half their height (`pill-28` to `pill-34`, `location`,
`search`), which sets them apart from the boxy controls. A new pill uses half its own
height; add it to the scale rather than rounding to the nearest step. Tabs are cards rounded only at the top so they join the toolbar below.
Icons are drawn for this project on a 24px grid: 1.8px round-capped strokes, no fills
except the menu dots, the pin and a saved bookmark's star.

## Components

### Buttons
Calm and exact; they confirm a press with a 1px dip, nothing more.
- **Shape:** gently rounded (6px), 32px tall, 12px horizontal padding.
- **Default:** Graphite Card fill, Hairline border, Instrument White text.
- **Quiet:** transparent with no border; used for toolbar, tab-strip, menu and list-row actions. Hover adds the text-colour wash.
- **Destructive:** the default button with Warning Coral (Warning Crimson on Light) text, and its border turns that colour on hover. Used for actions that delete or clear (Clear selected data…, Clear all history…, the confirmation's action). The label names the action.
- **Hover / Active / Focus:** hover brightens the border to Dial Grey; active applies the pressed wash and moves 1px down; focus shows the 2px signal ring. Transitions are 120ms ease and switch off under reduced motion.
- **Disabled:** no fill, no border, Disabled Grey text, including destructive (coral) buttons, so an unavailable Delete never looks armed.

### Navigation icon buttons
- Round (34px circle), transparent, an 18px stroke icon centred. Hover adds the wash. Reload swaps with Stop while loading. Every one has a label and its shortcut in the tooltip.

### Inputs / Fields
- **Style:** Graphite Field fill, Hairline border, 6px radius, 32px tall; placeholders use Dial Grey (`--secondary`, `opacity: 1`) so every theme meets 4.5:1 AA contrast; carets and text selection follow the theme signal (`caret-color: var(--accent)`, 28% signal mix). The new-tab search is a 48px pill with 17px text.
- **Focus:** 2px signal outline; the address line shows a signal border plus a 1px signal ring.
- **Checkboxes and progress:** native controls tinted with the signal via `accent-color`.

### The location line (signature component)
The one bold element on the panel. A 36px pill in Graphite Field with a Field Hairline
border containing, left to right: the **security badge** (on an encrypted page a 28px
round button holding only a Dial Grey lock, with the wash on hover; while a page loads
the same round button turns a quiet Dial Grey arc (still in reduced motion), named
"Connecting" for Narrator, so the origin beside it never moves; otherwise a 28px pill
with the text-colour wash, an icon and a 12px/600 word, coral on *Not encrypted* or
failure, Dial Grey and regular weight on browser pages), the **address** (an origin-emphasised view whose subdomains collapse
before the registrable domain tail on narrow bars, hiding on focus to show the editable
field), the **star** (30px round, Dial Grey at rest, filled in the theme signal when the
current page is bookmarked) and the **shield** (32px pill, shown only once an extension
that can or may block requests is added and never in a private window, because Rust
Browser has no ad blocker of its own; Dial Grey at rest, the text colour while an
extension can block, with its blocked-request count in Dial Grey 12px/600 tabular
figures: a tally, not a warning). The shield is always a plain outline with no check
mark, so it never reads as a verdict on the site next to the lock (GrayText in high
contrast while no extension can block).

### Tabs
- **Width:** every tab is 240px until the strip is full; then all shrink together to 64px before the strip scrolls, so a title change never moves another tab. After a close with the close button, every tab keeps its width while the pointer stays in the strip, so the next close button slides under the pointer; widths are released when the pointer leaves or a tab opens.
- **Inactive:** transparent cards, 36px tall; title in Dial Grey, fading out over its last 20px when it is too long (no ellipsis); a 1px hairline separates neighbouring inactive tabs; the drawn close button sits at 55% opacity until hovered, and on a tab narrower than 96px it appears only on hover.
- **Active:** takes Graphite Surface to join the toolbar, the signal rule along its top edge and a regular-weight 13px title in Instrument White. It keeps at least 120px however full the strip is, so the selected tab always shows its title, and it is scrolled back into view when the scroll arrows appear.
- **States:** a tab with no site icon shows the drawn globe; pinned tabs in the strip are a 44px card holding only the site icon, with no close button (middle-click, the tab menu's Close tab or Ctrl+W close them; their name stays on the tab for Narrator); sleeping tabs set their title in italic, and below 96px they drop the moon so the italic carries the state alone; a drop target shows a 3px signal rule on the insertion side.
- **Bookmark menus:** right-click (or Shift+F10 and the Menu key) on a bookmarks-bar button or a row in a folder's dropdown opens the **bookmark menu**, the tab menu's 220px rows and hairlines opened at the pointer (from a folder's dropdown it replaces that dropdown). A bookmark gets Open in new tab, new window and new private window; Edit bookmark… and Delete bookmark; Copy link; Add bookmark… and Add folder…; Manage bookmarks. A folder gets Open all in tabs (with its count once read), Rename folder… and Delete folder…, the same Add rows and Manage bookmarks. The rows ending in an ellipsis open a 340px **bookmark editor** where the menu was, in the star's form style; Delete folder… is a confirmation in the same panel with Cancel focused and Delete folder in the error colour.
- **Gestures:** middle-click closes any tab. Right-click (or Shift+F10 and the Menu key on a focused tab) opens the **tab menu**, a 220px dropdown under the tab that acts on that tab, not the selected one: Duplicate, Pin tab or Unpin tab, Move left and Move right (disabled at the end of the tab's pinned or unpinned group), then Close tab and Close other tabs (which keeps pinned tabs), then Reopen closed tab, with hairlines between the groups. The tab column has middle-click but no tab menu.
- **Vertical column:** full-width rows, 8px radius on all corners; the active tab is marked by a 3px signal bar with rounded ends, inset 9px from the row's top and bottom, so it never bends around the corners.

### Profile button and picker
- **Profile button:** a 32px round button left of the menu button holding a 24px avatar: the profile's initial in white, 600 weight, on the profile's colour. Shown only when there is more than one profile (Settings → Profiles adds one), and hidden in private windows.
- **Profile colours:** eight fixed swatches, each at least 4.5:1 under a white initial and kept clear of every theme's signal and error colours (at least 20° of hue or 0.3 of lightness apart, checked by `ui::tests`), so an avatar never reads as "active" or as a problem: Teal `#0E6F7C`, Green `#1F7A4D`, Purple `#6B3FA0`, Brown `#8A4B1F`, Magenta `#9C2C6E`, Olive `#5E6A1E`, Taupe `#6E5F52`, Slate `#56606C`. Lists saved with the first palette (which reused Light's cobalt and crimson) load with the nearest new colour. Swatches are 24px circles; the chosen one carries a 2px text-colour ring outside a 2px gap, and focus adds the signal outline beyond it, so both show at once. In high contrast, avatars and swatches keep their colour with a 1px ButtonText border.
- **Profile identity stays the avatar.** Nothing else in the chrome takes the profile's colour; the window title names the profile when there is more than one.
- **Profile dropdown:** a 300px dropdown with the current profile (40px avatar, name, "This window's profile") over a hairline, then the other profiles as quiet rows (avatar and name, 36px; 44px with a second Dial Grey line, "Open in another window", when that profile is already open, whose action switches to it rather than opening another window; Up and Down move between rows), then Manage profiles and Add profile. It opens with focus on its heading, so a stray Enter never opens another profile. Add profile always goes through the editor in Settings → Profiles (name and color first).
- **Settings → Profiles rows** say why a profile cannot be deleted ("This window", "The Default profile can't be deleted", "It is open in another window; close that window to delete it.") and disable Delete while that profile is open.
- **Who's browsing?:** a 640 × 460 window on Graphite Surface with a 26px/600 display heading and 124px tiles (Graphite Panel, Hairline border, 10px radius) holding a 56px avatar and the name; Add profile is the last tile with a dashed outline. The Default profile's theme applies.

### Dropdowns
- Graphite Panel, 1px Hairline, 10px radius, 8px padding (12px for the site, star and shield panels), the Float shadow. Only one is open at a time, over the live page.
- **Focus on opening:** menus (⋮, bookmark folders) focus their first item; a form (the star) its first field; every other dropdown (site information, shield, profile) its heading, which shows the signal focus ring to keyboard users only. A pending permission prompt focuses **Don't allow**.
- **A page covered by a full panel is not described.** While Settings or another full panel covers the page, the badge reads "Browser page", the address line shows the panel's name ("Settings"), and the star and shield are disabled. Settings carries no per-site readout; that lives in the site-information dropdown.
- **Menu (⋮):** labelled groups (New: New tab, New window, New private window and Reopen closed window; This page; This tab: Pin, Keep awake and Reopen closed tab, with moving tabs in the tab menu; Library; then Settings, Profiles… and Extensions…, which open Settings at those sections so the hidden profile button and shield can be found, and Keyboard shortcuts; then Exit on its own below a hairline), each with a 12px Dial Grey label and a hairline above it. Items are quiet full-width 32px buttons with the label at the left and the shortcut at the right in 12px Dial Grey. Zoom is one row: a label, then − / the current percentage / +. Keep tab awake and Exit show their state in that place instead of a shortcut (Exit: "Tabs return", "Tabs not kept" or, in a private window, "Nothing kept").
- **Permission request:** takes over the site-information dropdown as a modal (`role="dialog"`, `aria-modal="true"`, containing `Tab` / `Shift+Tab` inside the prompt) when a website asks, hiding the static connection details until answered so a misclick never dismisses the request. `#site-panel` itself takes a single 2px signal border (10px radius, 12px padding); a 14px/600 heading in plain words ("meet.example.com wants to use your camera"), the full origin, a Remember checkbox, a calm timeout notice that counts down only in the final 10 seconds, then Don't allow and Allow as two equal default buttons (Allow stays disabled during the initial 500ms double-click guard). Neither is styled as the primary action, so the design never nudges toward granting.

### Cards / Containers
- **Favourite card:** Graphite Surface, Hairline border, 10px radius, 10px × 12px padding, no shadow; a single horizontal row with a 32px tile (8px radius, Graphite Card, holding the site's icon or its first letter) and bold title on the left, and a quiet 28px `×` remove button on the right.
- **Library rows:** a hairline beneath each and a wash on hover. The whole title (14px/600, preceded by the bookmark's `16x16` site icon or letter monogram chip in Bookmarks, with a 12px Dial Grey time or folder and address beneath) is one quiet button that opens the entry; New tab, Edit and Remove follow as quiet Dial Grey buttons on the same line. History is grouped under `h2.day` 13px/600 Dial Grey day headings (Today, Yesterday, then the date).
- **Info rows** (downloads, extensions, remembered permissions): 12px × 8px padding, the same title and second line, actions at the right.

### Confirmation dialog
The browser's own modal `<dialog>` for anything destructive, never the system confirm box. Graphite Panel, 1px Hairline, 10px radius, 20px padding, the Float shadow, over a scrim; at most 440px wide. A 15px/600 question as the heading, one sentence on what happens, a list of what will be removed when there is a choice, a muted line on what is kept, then Cancel (focused) and a destructive button that names the action ("Clear data", "Revoke and close tabs"). Escape cancels. It closes, as a cancel, if its panel changes.

### Notices & Inline Validation
The line under the toolbar, the Settings status, and inline form validation lines (`.form-status`). Information (a completed action, progress) is set in the text colour with an info icon; a problem in the error colour with a warning icon, so the kind never depends on colour. Inline form validation keeps user input intact and marks the offending field with `aria-invalid="true"`. Anything not marked as information is a problem. 12px on an 18px line, up to two lines with the full text in the tooltip, and a drawn 28px dismiss button.

### Windows high contrast
Under `forced-colors`, every colour yields to system colours: icons stroke in ButtonText, focus becomes a 3px Highlight outline, and the active tab gains a Highlight outline and an underlined title so selection never depends on colour.

## Do's and Don'ts

### Do:
- **Do** use the theme variables for every colour so Dark, Slate, Plum, Light and high contrast all work, and keep their names stable for the contrast tests.
- **Do** keep the signal colour for state: selected, focused, loading, private, a saved bookmark, a pending permission decision.
- **Do** ask before anything destructive in the browser's confirmation dialog, with Cancel focused and a button that names the action.
- **Do** keep permission choices equal in weight, and let only Allow grant: closing or waiting answers Don't allow.
- **Do** give every icon-only control an accessible name and put its keyboard shortcut in the tooltip.
- **Do** write state in plain words next to its icon or colour (*Not encrypted*, *Private*, a count).
- **Do** keep controls 32px tall (34px round navigation buttons, 36px location line) and spacing on the 4px scale.
- **Do** draw new icons on the 24px grid with 1.8px round-capped strokes.
- **Do** provide a reduced-motion fallback and a forced-colours fallback for every new state cue.

### Don't:
- **Don't** add a second accent colour or use the signal for decoration, solid large fills or illustration (the Glow background's 10% tint is the only area use).
- **Don't** put shadows on cards, buttons, tabs or panels; only floating dropdowns, menus and the confirmation dialog get the Float shadow.
- **Don't** use the error colour for anything that is not a problem or a destructive action.
- **Don't** load fonts, images or anything else from a website or a CDN into the chrome; icons are inline SVG and favicons arrive as checked data URLs.
- **Don't** dim, freeze or replace the page behind a dropdown; it stays live (see the Live-Page Rule).
- **Don't** imitate Fluent or Edge controls outright, and don't break Windows behaviour (title bar, Snap, Narrator, scaling) for the sake of the look.
- **Don't** set anything else in the toolbar larger than the address origin, or at 600 weight and 15px or more.
