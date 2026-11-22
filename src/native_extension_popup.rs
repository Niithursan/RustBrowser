//! An extension's own toolbar-button page (its popup), shown under the shield.
//!
//! WebView2 runs extensions but has no toolbar, so it never shows their popups.
//! Rust Browser loads the popup page (`chrome-extension://<id>/<default_popup>`)
//! in a small view of its own in the normal website profile, like a browser's
//! extension popup. WebView2 treats every view as a tab in a window of its own,
//! so a popup asking for "the active tab of the current window" would get itself;
//! a script added before the popup's own scripts answers that question with the
//! page the shield belongs to instead (see `shim`).
//!
//! The popup page is the extension's own code with the extension's own rights,
//! exactly as in other browsers. It can only navigate within its extension and
//! cannot open windows; it can send the browser its size, and a website address
//! it asks to open, which opens in a new tab like any link.
use crate::app::Event;
use webview2_com::{FocusChangedEventHandler, WindowCloseRequestedEventHandler};
use winit::{event_loop::EventLoopProxy, window::Window};
use wry::{Rect, WebContext, WebView, WebViewBuilder, WebViewBuilderExtWindows, WebViewExtWindows};

/// The popup's first width before it reports its own (logical pixels). Pages
/// with a minimum width report a larger one.
pub const START_WIDTH: f64 = 320.0;
/// The popup's first height: small, so a page that fills its view reports the
/// height of its content rather than of the view.
pub const START_HEIGHT: f64 = 120.0;
/// Size limits, as in other browsers.
pub const MIN_SIZE: f64 = 25.0;
pub const MAX_WIDTH: f64 = 800.0;
pub const MAX_HEIGHT: f64 = 600.0;

/// What a popup view reports to the browser.
#[derive(Debug)]
pub enum Report {
    /// The size its content asks for, already within the limits.
    Size(f64, f64),
    /// It asked to close (`window.close()`, Escape).
    Close,
    /// It lost focus (a press anywhere else): it closes, as in other browsers.
    Blur,
    /// It asked to open a website (`chrome.tabs.create`, a link to a new window):
    /// it opens in a new tab. Only HTTP(S) addresses are passed on.
    Link(String),
}

/// The address of `extension`'s popup page `path`, when both are valid.
pub fn address(extension: &str, path: &str) -> Option<String> {
    (crate::native_extensions::valid_id(extension)
        && path.is_ascii()
        && crate::native_extensions::action_popup(&serde_json::json!({
            "action": {"default_popup": path}
        }))
        .as_deref()
            == Some(path))
    .then(|| format!("chrome-extension://{extension}/{path}"))
}

/// A size message from the popup (`size:<width>,<height>` in CSS pixels),
/// clamped to the limits. Anything else is refused.
pub fn size_message(body: &str) -> Option<(f64, f64)> {
    let (width, height) = body.strip_prefix("size:")?.split_once(',')?;
    let parse = |text: &str| {
        (text.len() <= 8)
            .then(|| text.parse::<f64>().ok())
            .flatten()
            .filter(|v| v.is_finite())
    };
    Some((
        parse(width)?.clamp(MIN_SIZE, MAX_WIDTH),
        parse(height)?.clamp(MIN_SIZE, MAX_HEIGHT),
    ))
}

/// Where a popup of `size` goes: its right edge under `anchor_right`, its top at
/// `anchor_top`, kept inside a window of `bounds` (all logical pixels).
pub fn place(anchor_right: f64, anchor_top: f64, size: (f64, f64), bounds: (f64, f64)) -> Rect {
    let (width, height) = (size.0.min(bounds.0).max(1.0), size.1);
    let x = (anchor_right - width).clamp(0.0, (bounds.0 - width).max(0.0));
    let y = anchor_top.clamp(0.0, bounds.1.max(0.0));
    let height = height.min((bounds.1 - y).max(1.0));
    Rect {
        position: winit::dpi::LogicalPosition::new(x, y).into(),
        size: winit::dpi::LogicalSize::new(width, height).into(),
    }
}

/// The script added to the popup before its own. `page` is the address of the
/// page the popup acts on and `time_origin` that document's
/// `performance.timeOrigin`, which tells two tabs with the same address apart.
///
/// It changes three things, all inside the popup page:
/// - `chrome.tabs.query({active: true, currentWindow: true})` (or
///   `lastFocusedWindow`) answers with that page's tab instead of the popup;
/// - Escape closes the popup, as in other browsers;
/// - the page's size is sent to the browser so the view fits its content.
pub fn shim(page: &str, time_origin: Option<f64>) -> String {
    // JSON, with `<` escaped, is a safe JavaScript literal for any address.
    let page = serde_json::to_string(page)
        .unwrap_or_else(|_| "\"\"".into())
        .replace('<', "\\u003c");
    let origin = time_origin
        .filter(|t| t.is_finite())
        .map_or_else(|| "null".to_string(), |t| t.to_string());
    format!(
        r#"(()=>{{'use strict';
const PAGE={page},ORIGIN={origin};
const api=globalThis.chrome,tabs=api&&api.tabs;
if(tabs&&typeof tabs.query==='function'){{
 const query=tabs.query.bind(tabs);let found=null;
 const find=async()=>{{
  if(found)return found;
  const same=(await query({{}})).filter(t=>t.url===PAGE);
  let pick=same.length===1?same[0]:null;
  if(!pick&&same.length>1&&ORIGIN!==null&&api.scripting){{
   for(const t of same){{try{{const[r]=await api.scripting.executeScript({{target:{{tabId:t.id}},func:()=>performance.timeOrigin}});if(r&&r.result===ORIGIN){{pick=t;break}}}}catch(e){{}}}}
  }}
  found=pick||same[0]||null;return found}};
 const wanted=i=>!!i&&i.active===true&&(i.currentWindow===true||i.lastFocusedWindow===true);
 const replaced=function(info,callback){{
  if(!wanted(info))return query(info,callback);
  const answer=find().then(t=>t?[Object.assign({{}},t,{{active:true}})]:[]);
  if(typeof callback==='function'){{answer.then(callback);return}}
  return answer}};
 try{{Object.defineProperty(tabs,'query',{{value:replaced,configurable:true,writable:true}})}}catch(e){{try{{tabs.query=replaced}}catch(e){{}}}}
}}
addEventListener('keydown',e=>{{if(e.key==='Escape'&&!e.defaultPrevented)window.close()}});
let last='';
const report=()=>{{const b=document.body;if(!b||!window.ipc)return;
 const style=getComputedStyle(b),r=b.getBoundingClientRect();
 const w=Math.ceil(Math.max(document.documentElement.scrollWidth,r.right+(parseFloat(style.marginRight)||0)));
 const h=Math.ceil(r.bottom+(parseFloat(style.marginBottom)||0));
 const m='size:'+w+','+h;if(m!==last){{last=m;window.ipc.postMessage(m)}}}};
addEventListener('DOMContentLoaded',()=>{{report();if(document.body)new ResizeObserver(report).observe(document.body)}});
addEventListener('load',report);
}})();"#
    )
}

/// Build the popup view for `address`, hidden, in the normal website profile.
/// It shows itself once it reports a size (see `Report`).
pub fn open(
    window: &Window,
    context: &mut WebContext,
    proxy: EventLoopProxy<Event>,
    ticket: u64,
    address: &str,
    script: &str,
    bounds: Rect,
) -> Result<WebView, Box<dyn std::error::Error>> {
    // The popup may move between its extension's own pages, nowhere else.
    let base = address
        .rfind('/')
        .map_or(address, |end| &address[..=end])
        .to_string();
    let extension = base
        .strip_prefix("chrome-extension://")
        .and_then(|rest| rest.split('/').next())
        .map(|id| format!("chrome-extension://{id}/"))
        .ok_or("not an extension page")?;
    let p = proxy.clone();
    let view = WebViewBuilder::new_with_web_context(context)
        .with_additional_browser_args("")
        .with_browser_extensions_enabled(true)
        .with_clipboard(false)
        .with_devtools(false)
        .with_hotkeys_zoom(false)
        .with_browser_accelerator_keys(false)
        .with_permission_handler(|_| wry::PermissionResponse::Deny)
        .with_download_started_handler(|_, _| false)
        .with_navigation_handler(move |url| url.starts_with(&extension))
        .with_new_window_req_handler({
            let p = proxy.clone();
            move |url, _| {
                // WebView2 hands the popup's `chrome.tabs.create` here.
                if url.len() <= 8192 && rust_browser::navigation::allowed(&url) {
                    let _ = p.send_event(Event::ExtensionPopup(ticket, Report::Link(url)));
                }
                wry::NewWindowResponse::Deny
            }
        })
        .with_initialization_script(script)
        .with_ipc_handler(move |request| {
            if let Some((width, height)) = size_message(request.body()) {
                let _ = p.send_event(Event::ExtensionPopup(ticket, Report::Size(width, height)));
            }
        })
        .with_bounds(bounds)
        .with_visible(false)
        .with_focused(false)
        .with_url(address)
        .build_as_child(window)?;
    // SAFETY: registration on this new view's controller and core, on the UI
    // thread that owns them; the handlers keep only an event proxy.
    unsafe {
        let mut token = 0;
        let p = proxy.clone();
        view.webview().add_WindowCloseRequested(
            &WindowCloseRequestedEventHandler::create(Box::new(move |_, _| {
                let _ = p.send_event(Event::ExtensionPopup(ticket, Report::Close));
                Ok(())
            })),
            &mut token,
        )?;
        // A press anywhere outside the popup takes focus from it and closes it.
        view.controller().add_LostFocus(
            &FocusChangedEventHandler::create(Box::new(move |_, _| {
                let _ = proxy.send_event(Event::ExtensionPopup(ticket, Report::Blur));
                Ok(())
            })),
            &mut token,
        )?;
    }
    Ok(view)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_valid_extension_page_is_addressed() {
        let id = "himjhdcnbdhjhebgefcgmneheaejdbim";
        assert_eq!(
            address(id, "popup.html").as_deref(),
            Some("chrome-extension://himjhdcnbdhjhebgefcgmneheaejdbim/popup.html")
        );
        assert_eq!(address("not-an-id", "popup.html"), None);
        assert_eq!(address(id, "../popup.html"), None);
        assert_eq!(address(id, "./popup.html"), None);
        assert_eq!(address(id, "popup.html?x"), None);
    }

    #[test]
    fn only_size_messages_within_limits_are_read() {
        assert_eq!(size_message("size:340,480"), Some((340.0, 480.0)));
        assert_eq!(size_message("size:5,5000"), Some((MIN_SIZE, MAX_HEIGHT)));
        assert_eq!(size_message("size:9999,10"), Some((MAX_WIDTH, MIN_SIZE)));
        for bad in [
            "",
            "size:",
            "size:1",
            "size:a,b",
            "size:NaN,1",
            "size:inf,1",
            "size:1,2,3",
            "close",
            "size:123456789,1",
        ] {
            assert_eq!(size_message(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_popup_stays_inside_the_window_under_its_button() {
        let at = |rect: Rect, scale: f64| {
            let p = rect.position.to_logical::<f64>(scale);
            let s = rect.size.to_logical::<f64>(scale);
            (p.x, p.y, s.width, s.height)
        };
        // Right-aligned under the button.
        assert_eq!(
            at(place(1800.0, 76.0, (360.0, 400.0), (1920.0, 1040.0)), 1.0),
            (1440.0, 76.0, 360.0, 400.0)
        );
        // Never off the left edge, never taller than the room below the button.
        assert_eq!(
            at(place(200.0, 76.0, (360.0, 600.0), (1920.0, 500.0)), 1.0),
            (0.0, 76.0, 360.0, 424.0)
        );
        // A window narrower than the popup.
        assert_eq!(
            at(place(300.0, 76.0, (360.0, 300.0), (320.0, 900.0)), 1.0),
            (0.0, 76.0, 320.0, 300.0)
        );
    }

    #[test]
    fn the_page_address_cannot_escape_the_shim() {
        let script = shim("https://a.test/</script><script>alert(1)//\"'", Some(1.5));
        assert!(!script.contains("</script>"));
        assert!(script.contains(r#"const PAGE="https://a.test/\u003c/script>"#));
        assert!(script.contains("ORIGIN=1.5;"));
        assert!(shim("https://a.test/", None).contains("ORIGIN=null;"));
        assert!(shim("https://a.test/", Some(f64::NAN)).contains("ORIGIN=null;"));
    }
}
