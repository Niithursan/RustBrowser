use servo::{CompositionEvent, CompositionState, ImeEvent, InputEvent, WheelDelta, WheelMode};
use winit::event::{Ime, MouseScrollDelta};

pub fn ime_events(event: Ime, active: bool) -> Vec<InputEvent> {
    let composition =
        |state, data| InputEvent::Ime(ImeEvent::Composition(CompositionEvent { state, data }));
    match event {
        Ime::Enabled => vec![composition(CompositionState::Start, String::new())],
        Ime::Preedit(text, _) => vec![composition(CompositionState::Update, text)],
        Ime::Commit(text) => vec![
            composition(CompositionState::Update, text.clone()),
            composition(CompositionState::End, text),
        ],
        Ime::Disabled if active => vec![InputEvent::Ime(ImeEvent::Dismissed)],
        Ime::Disabled => Vec::new(),
    }
}

pub fn wheel_delta(delta: MouseScrollDelta) -> WheelDelta {
    // Follow the pinned servoshell route. Converted line distances are pixels.
    let (x, y) = match delta {
        MouseScrollDelta::LineDelta(x, y) => (f64::from(x) * 76.0, f64::from(y) * 76.0),
        MouseScrollDelta::PixelDelta(point) => (point.x, point.y),
    };
    WheelDelta {
        x,
        y,
        z: 0.0,
        mode: WheelMode::DeltaPixel,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_composition_has_update_and_commit_without_losing_text() {
        let events = ime_events(Ime::Commit("日本語🙂".into()), true);
        assert_eq!(events.len(), 2);
        assert!(
            matches!(&events[0], InputEvent::Ime(ImeEvent::Composition(CompositionEvent {
            state: CompositionState::Update, data
        })) if data == "日本語🙂")
        );
        assert!(
            matches!(&events[1], InputEvent::Ime(ImeEvent::Composition(CompositionEvent {
            state: CompositionState::End, data
        })) if data == "日本語🙂")
        );
    }

    #[test]
    fn stale_ime_disable_does_not_blur_a_new_focus_target() {
        assert!(ime_events(Ime::Disabled, false).is_empty());
        assert!(matches!(
            ime_events(Ime::Disabled, true).as_slice(),
            [InputEvent::Ime(ImeEvent::Dismissed)]
        ));
        assert!(matches!(
            ime_events(Ime::Enabled, false).as_slice(),
            [InputEvent::Ime(ImeEvent::Composition(CompositionEvent {
                state: CompositionState::Start,
                ..
            }))]
        ));
        assert!(matches!(
            ime_events(Ime::Preedit("a".into(), Some((0, 1))), true).as_slice(),
            [InputEvent::Ime(ImeEvent::Composition(CompositionEvent {
                state: CompositionState::Update,
                ..
            }))]
        ));
    }

    #[test]
    fn pointer_wheel_pixel_distances_are_not_marked_as_lines() {
        let delta = wheel_delta(MouseScrollDelta::LineDelta(1.0, -2.0));
        assert_eq!((delta.x, delta.y), (76.0, -152.0));
        assert!(matches!(delta.mode, WheelMode::DeltaPixel));
        let delta = wheel_delta(MouseScrollDelta::PixelDelta(
            winit::dpi::PhysicalPosition::new(3.0, 4.0),
        ));
        assert_eq!((delta.x, delta.y), (3.0, 4.0));
    }

    #[test]
    fn empty_preedit_and_cancel_do_not_recommit_old_text() {
        let events = ime_events(Ime::Preedit(String::new(), None), true);
        assert!(matches!(events.as_slice(),
            [InputEvent::Ime(ImeEvent::Composition(CompositionEvent {
                state: CompositionState::Update, data
            }))] if data.is_empty()));
        assert!(matches!(
            ime_events(Ime::Disabled, true).as_slice(),
            [InputEvent::Ime(ImeEvent::Dismissed)]
        ));
        let committed = ime_events(Ime::Commit(String::new()), true);
        assert!(
            matches!(&committed[1], InputEvent::Ime(ImeEvent::Composition(CompositionEvent {
            state: CompositionState::End, data
        })) if data.is_empty())
        );
    }
}
