use rust_browser::settings::Theme;
use winit::{
    platform::windows::{Color, WindowExtWindows},
    window::Window,
};

fn colors(theme: Theme) -> ([u8; 3], [u8; 3]) {
    match theme {
        Theme::Dark => ([0x13, 0x15, 0x18], [0xE9, 0xEC, 0xEF]),
        Theme::Light => ([0xD9, 0xDE, 0xE3], [0x1B, 0x24, 0x30]),
        Theme::Slate => ([0x14, 0x1C, 0x23], [0xE9, 0xEC, 0xEF]),
        Theme::Plum => ([0x1B, 0x14, 0x20], [0xE9, 0xEC, 0xEF]),
    }
}

pub fn apply_theme(window: &Window, theme: Theme) {
    let (background, foreground) = colors(theme);
    let background = Color::from_rgb(background[0], background[1], background[2]);
    window.set_title_background_color(Some(background));
    window.set_border_color(Some(background));
    window.set_title_text_color(Color::from_rgb(foreground[0], foreground[1], foreground[2]));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caption_palette_matches_each_toolbar_theme() {
        assert_eq!(
            colors(Theme::Dark),
            ([0x13, 0x15, 0x18], [0xE9, 0xEC, 0xEF])
        );
        assert_eq!(
            colors(Theme::Light),
            ([0xD9, 0xDE, 0xE3], [0x1B, 0x24, 0x30])
        );
        assert_eq!(
            colors(Theme::Slate),
            ([0x14, 0x1C, 0x23], [0xE9, 0xEC, 0xEF])
        );
        assert_eq!(
            colors(Theme::Plum),
            ([0x1B, 0x14, 0x20], [0xE9, 0xEC, 0xEF])
        );
    }
}
