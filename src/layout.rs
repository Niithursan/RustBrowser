//! Where a browser window's views go: the toolbar, the webpage, the vertical tab
//! column and the library column. Engine-independent, so every front end places
//! its views by the same table.
use std::time::Duration;

/// A rectangle with rounded corners, in logical pixels relative to a view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipArea {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub radius: f64,
}

/// Width of the vertical tab column, in logical pixels.
pub const TABS_WIDTH: f64 = 240.0;
/// Default, smallest and largest library column width, in logical pixels.
pub const LIBRARY_WIDTH: f64 = 360.0;
pub const LIBRARY_MIN: f64 = 260.0;
pub const LIBRARY_MAX: f64 = 640.0;
/// The page keeps at least this much width beside the columns.
pub const PAGE_MIN: f64 = 320.0;
/// How long a page entering fullscreen is named at the top of the screen.
pub const FULLSCREEN_NOTICE: Duration = Duration::from_secs(4);
/// The fullscreen notice: its widest width, its height and its distance from the top.
pub const FULLSCREEN_NOTICE_WIDTH: f64 = 440.0;
pub const FULLSCREEN_NOTICE_HEIGHT: f64 = 44.0;
pub const FULLSCREEN_NOTICE_TOP: f64 = 16.0;

/// Where the fullscreen notice is drawn in a toolbar view `width` wide: centred
/// at the top, at most `FULLSCREEN_NOTICE_WIDTH` and 16 pixels from each side.
pub fn fullscreen_notice_area(width: f64) -> ClipArea {
    let w = FULLSCREEN_NOTICE_WIDTH.min(width - 32.0).max(1.0);
    ClipArea {
        x: ((width - w) / 2.0).max(0.0).floor(),
        y: FULLSCREEN_NOTICE_TOP,
        width: w,
        height: FULLSCREEN_NOTICE_HEIGHT,
        radius: 8.0,
    }
}

/// The host a fullscreen notice names for a page at `url`.
pub fn fullscreen_host(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .filter(|host| !host.is_empty())
        .unwrap_or_else(|| "This page".into())
}
/// Where each view of a window goes. No two shown views ever overlap: the toolbar and
/// pages are separate windows whose stacking order Windows does not keep.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub toolbar: (f64, f64, f64, f64),
    pub page: Option<(f64, f64, f64, f64)>,
    pub tabs: Option<(f64, f64, f64, f64)>,
    pub library: Option<(f64, f64, f64, f64)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Full {
    /// A webpage is shown below the toolbar.
    None,
    /// The new-tab page fills the toolbar's column; the library may still sit beside it.
    NewTab,
    /// A full panel (Settings), or a dropdown over one, covers everything except the tab column.
    Cover,
    /// A dropdown over a webpage: the page and library stay where they are, and the
    /// toolbar spans its column above them, clipped to the chrome and the dropdown.
    Dropdown,
    /// A fullscreen page (a video, a game) fills its column and the toolbar is
    /// hidden (zero height). Escape, which always reaches the page, leaves it.
    Bare,
}

/// The layout table (see ARCHITECTURE.md; shared by the Windows and Linux front ends). In `Full::Cover` (Settings or an open
/// dropdown), the toolbar covers its whole column and the page and library are hidden.
/// In `Full::NewTab`, the toolbar fills the height of its column and shrinks its width
/// when the library column is open beside it.
pub fn layout(
    width: f64,
    height: f64,
    chrome: f64,
    vertical: bool,
    library: Option<f64>,
    full: Full,
) -> Layout {
    let left = if vertical {
        TABS_WIDTH.min(width / 2.0)
    } else {
        0.0
    };
    let rest = (width - left).max(1.0);
    let lib = library
        .filter(|_| full != Full::Cover)
        .map(|w| {
            w.clamp(LIBRARY_MIN, LIBRARY_MAX)
                .min((rest - PAGE_MIN).max(0.0))
        })
        .filter(|w| *w >= LIBRARY_MIN.min(rest / 2.0) && *w > 0.0);
    let chrome = if full == Full::Bare { 0.0 } else { chrome };
    let body = (height - chrome).max(1.0);
    let (toolbar_w, toolbar_h) = match full {
        Full::None => (rest, chrome),
        Full::Bare => (rest, 0.0),
        Full::NewTab => ((rest - lib.unwrap_or(0.0)).max(1.0), height),
        Full::Cover | Full::Dropdown => (rest, height),
    };
    Layout {
        toolbar: (left, 0.0, toolbar_w, toolbar_h),
        page: matches!(full, Full::None | Full::Dropdown | Full::Bare).then_some((
            left,
            chrome,
            (rest - lib.unwrap_or(0.0)).max(1.0),
            body,
        )),
        tabs: vertical.then_some((0.0, 0.0, left, height)),
        library: lib.map(|w| match full {
            Full::NewTab => (width - w, 0.0, w, height),
            _ => (width - w, chrome, w, body),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The layout table never lets two shown views overlap and keeps every view inside
    /// the window, in every combination of size, tab column, library and full panel.
    #[test]
    fn shown_views_never_overlap() {
        let overlap = |a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)| {
            a.0 < b.0 + b.2 - 0.5
                && b.0 < a.0 + a.2 - 0.5
                && a.1 < b.1 + b.3 - 0.5
                && b.1 < a.1 + a.3 - 0.5
        };
        let mut cases = 0;
        for width in [560.0, 800.0, 1200.0, 1920.0, 2560.0] {
            for height in [320.0, 820.0, 1440.0] {
                for chrome in [80.0, 119.0, 160.0] {
                    for vertical in [false, true] {
                        for library in [None, Some(200.0), Some(360.0), Some(900.0)] {
                            for full in [Full::None, Full::NewTab, Full::Cover, Full::Bare] {
                                let plan = layout(width, height, chrome, vertical, library, full);
                                let shown: Vec<_> =
                                    [Some(plan.toolbar), plan.page, plan.tabs, plan.library]
                                        .into_iter()
                                        .flatten()
                                        .collect();
                                for (i, a) in shown.iter().enumerate() {
                                    assert!(
                                        a.0 >= -0.5
                                            && a.1 >= -0.5
                                            && a.0 + a.2 <= width + 0.5
                                            && a.1 + a.3 <= height + 0.5,
                                        "outside the window: {a:?} in {width}x{height}"
                                    );
                                    for b in &shown[i + 1..] {
                                        assert!(
                                            !overlap(*a, *b),
                                            "{a:?} overlaps {b:?} ({width}x{height}, vertical {vertical}, library {library:?}, full {full:?})"
                                        );
                                    }
                                }
                                // Full::None and Full::Bare show the webpage; Full::Cover hides the library too.
                                assert_eq!(
                                    plan.page.is_some(),
                                    matches!(full, Full::None | Full::Bare)
                                );
                                if full == Full::Cover {
                                    assert!(plan.library.is_none());
                                }
                                assert_eq!(plan.tabs.is_some(), vertical);
                                cases += 1;
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(cases, 5 * 3 * 3 * 2 * 4 * 4);
    }

    /// A dropdown over a webpage moves nothing: the page and library keep their
    /// places (and keep running), and only the toolbar grows down its column to
    /// hold the dropdown. The toolbar is then clipped (see `clip_toolbar`).
    #[test]
    fn a_dropdown_keeps_the_page_in_place() {
        for vertical in [false, true] {
            for library in [None, Some(360.0)] {
                let page = layout(1200.0, 820.0, 119.0, vertical, library, Full::None);
                let menu = layout(1200.0, 820.0, 119.0, vertical, library, Full::Dropdown);
                assert_eq!(menu.page, page.page);
                assert_eq!(menu.library, page.library);
                assert_eq!(menu.tabs, page.tabs);
                assert_eq!(menu.toolbar.0, page.toolbar.0);
                assert_eq!(menu.toolbar.2, page.toolbar.2);
                assert_eq!(menu.toolbar.3, 820.0);
            }
        }
    }

    /// The fullscreen notice is centred near the top and never wider than the
    /// window less 16 pixels a side; it names the page's host.
    #[test]
    fn the_fullscreen_notice_names_the_site_at_the_top() {
        let wide = fullscreen_notice_area(1920.0);
        assert_eq!(
            (wide.x, wide.y, wide.width, wide.height),
            (740.0, 16.0, 440.0, 44.0)
        );
        let narrow = fullscreen_notice_area(300.0);
        assert_eq!((narrow.x, narrow.width), (16.0, 268.0));
        assert_eq!(
            fullscreen_host("https://www.youtube.com/watch?v=x"),
            "www.youtube.com"
        );
        assert_eq!(fullscreen_host("http://127.0.0.1:8080/a"), "127.0.0.1");
        assert_eq!(fullscreen_host("about:blank"), "This page");
        assert_eq!(fullscreen_host("not a url"), "This page");
    }

    /// A fullscreen page (a video) fills the whole window: the toolbar is hidden.
    #[test]
    fn a_fullscreen_page_fills_the_window() {
        let plan = layout(1920.0, 1080.0, 119.0, false, None, Full::Bare);
        assert_eq!(plan.page, Some((0.0, 0.0, 1920.0, 1080.0)));
        assert_eq!(plan.toolbar.3, 0.0);
        assert!(plan.tabs.is_none() && plan.library.is_none());
    }

    #[test]
    fn the_library_keeps_room_for_the_page() {
        let plan = layout(1200.0, 820.0, 119.0, true, Some(LIBRARY_MAX), Full::None);
        let page = plan.page.unwrap();
        assert!(page.2 >= PAGE_MIN, "page width {}", page.2);
        let newtab = layout(1200.0, 820.0, 119.0, true, Some(LIBRARY_MAX), Full::NewTab);
        assert!(newtab.page.is_none());
        assert!(newtab.library.is_some());
        assert!(
            newtab.toolbar.2 >= PAGE_MIN,
            "newtab width {}",
            newtab.toolbar.2
        );
        let narrow = layout(560.0, 820.0, 119.0, true, Some(LIBRARY_WIDTH), Full::None);
        assert!(narrow.page.unwrap().2 >= 1.0);
    }
}
