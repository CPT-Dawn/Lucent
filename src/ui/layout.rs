//! Stack geometry: where each Popup sits, and how big the layer-shell
//! window around the stack is. All Popups share one transparent window,
//! padded so the card shadows aren't clipped; the input region is cut down
//! to the cards themselves.

use lucent::config::{Horizontal, Position, Vertical};

use super::theme::Insets;

/// Where an entering Popup slides in from (and a leaving one slides to),
/// relative to its place: from the anchored side.
pub fn enter_offset(position: Position) -> (f64, f64) {
    match (position.horizontal, position.vertical) {
        (Horizontal::Right, _) => (28.0, 0.0),
        (Horizontal::Left, _) => (-28.0, 0.0),
        (Horizontal::Center, Vertical::Top) => (0.0, -18.0),
        (Horizontal::Center, Vertical::Bottom) => (0.0, 18.0),
    }
}

/// The screen edges the window is anchored to, as 1 (anchored) or 0.
fn anchored(position: Position) -> Insets {
    Insets {
        top: i32::from(position.vertical == Vertical::Top),
        bottom: i32::from(position.vertical == Vertical::Bottom),
        left: i32::from(position.horizontal == Horizontal::Left),
        right: i32::from(position.horizontal == Horizontal::Right),
    }
}

/// Each card's distance from the anchored edge of the stack, for heights
/// given newest first: the newest sits nearest the edge.
pub fn offsets(heights: &[i32], gap: i32) -> Vec<i32> {
    heights
        .iter()
        .scan(0, |next, height| {
            let offset = *next;
            *next += height + gap;
            Some(offset)
        })
        .collect()
}

/// The Overflow pill's distance from the anchored edge: one gap past the
/// oldest card (heights newest first).
pub fn pill_offset(heights: &[i32], gap: i32) -> i32 {
    heights.iter().map(|height| height + gap).sum()
}

/// Which of the outputs (by connector name, e.g. `DP-1`) the stack is
/// pinned to. `None` lets the compositor choose, which is the focused
/// output: when no output is named, or the named one isn't connected.
pub fn pick_output(wanted: Option<&str>, connectors: &[Option<String>]) -> Option<usize> {
    let wanted = wanted?;
    connectors
        .iter()
        .position(|connector| connector.as_deref() == Some(wanted))
}

/// Room around the cards inside the window: the shadow's reach, except
/// that towards an anchored screen edge there is no point reaching past
/// the screen, so the padding there is at most the margin.
pub fn padding(position: Position, shadow_reach: Insets, margin: i32) -> Insets {
    let anchored = anchored(position);
    let side = |reach: i32, on: i32| if on == 1 { reach.min(margin) } else { reach };
    Insets {
        top: side(shadow_reach.top, anchored.top),
        right: side(shadow_reach.right, anchored.right),
        bottom: side(shadow_reach.bottom, anchored.bottom),
        left: side(shadow_reach.left, anchored.left),
    }
}

/// Layer-shell margins that put the cards `margin` from the anchored
/// edges, given the window's padding.
pub fn window_margins(position: Position, padding: Insets, margin: i32) -> Insets {
    let anchored = anchored(position);
    let side = |pad: i32, on: i32| if on == 1 { (margin - pad).max(0) } else { 0 };
    Insets {
        top: side(padding.top, anchored.top),
        right: side(padding.right, anchored.right),
        bottom: side(padding.bottom, anchored.bottom),
        left: side(padding.left, anchored.left),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAWN_REACH: Insets = Insets {
        top: 8,
        right: 26,
        bottom: 44,
        left: 26,
    };

    #[test]
    fn the_newest_popup_sits_at_the_edge_and_older_ones_follow() {
        assert_eq!(offsets(&[80, 120, 60], 10), [0, 90, 220]);
        assert!(offsets(&[], 10).is_empty());
    }

    #[test]
    fn the_pill_follows_the_oldest_popup_one_gap_further_from_the_edge() {
        assert_eq!(pill_offset(&[80, 120, 60], 10), 290);
        assert_eq!(pill_offset(&[], 10), 0);
    }

    #[test]
    fn a_named_output_pins_the_stack_and_no_name_follows_focus() {
        let outputs = [Some("eDP-1".to_string()), None, Some("DP-1".to_string())];

        assert_eq!(pick_output(Some("DP-1"), &outputs), Some(2));
        assert_eq!(pick_output(Some("HDMI-A-1"), &outputs), None);
        assert_eq!(pick_output(None, &outputs), None);
    }

    #[test]
    fn popups_slide_in_from_the_anchored_side() {
        let at = |vertical, horizontal| {
            enter_offset(Position {
                vertical,
                horizontal,
            })
        };
        assert_eq!(at(Vertical::Top, Horizontal::Right), (28.0, 0.0));
        assert_eq!(at(Vertical::Bottom, Horizontal::Left), (-28.0, 0.0));
        assert_eq!(at(Vertical::Top, Horizontal::Center), (0.0, -18.0));
        assert_eq!(at(Vertical::Bottom, Horizontal::Center), (0.0, 18.0));
    }

    #[test]
    fn top_right_cards_sit_exactly_margin_from_the_corner() {
        let position = Position::default();
        let pad = padding(position, DAWN_REACH, 16);
        let margins = window_margins(position, pad, 16);

        assert_eq!(
            pad,
            Insets {
                top: 8,
                right: 16,
                bottom: 44,
                left: 26,
            }
        );
        assert_eq!(margins.top + pad.top, 16);
        assert_eq!(margins.right + pad.right, 16);
        assert_eq!((margins.left, margins.bottom), (0, 0));
    }
}
