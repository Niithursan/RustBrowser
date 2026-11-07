use serde::{Deserialize, Serialize};

pub const HEIGHT: f64 = 100.0;
pub const HTML: &str = include_str!("toolbar.html");

#[derive(Debug, Deserialize)]
#[serde(tag = "action", content = "value", rename_all = "snake_case")]
pub enum Command {
    Navigate(String),
    Back,
    Forward,
    Reload,
    Stop,
    FocusAddress,
    FocusPage,
    Home,
    Ready,
}

#[derive(Default, Serialize)]
pub struct State {
    pub url: String,
    pub loading: bool,
    pub back: bool,
    pub forward: bool,
    pub message: String,
}
