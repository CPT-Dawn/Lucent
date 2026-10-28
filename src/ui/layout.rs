//! Stack geometry: where each Popup sits, and how big the layer-shell
//! window around the stack is. All Popups share one transparent window,
//! padded so the card shadows aren't clipped; the input region is cut down
//! to the cards themselves.

use super::theme::Insets;

/// Which screen edge the stack hugs vertically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Vertical {
    #[default]
    Top,
    Bottom,
}

/// Where the stack sits horizontally.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Horizontal {
    Left,
    Center,
    #[default]
    Right,
}

/// Where Popups appear on screen (`popup.position`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Anchor {
    pub vertical: Vertical,
    pub horizontal: Horizontal,
}

impl Anchor {
    /// Where an entering Popup slides in from (and a leaving one slides
    /// to), relative to its place: from the anchored side.
    pub fn enter_offset(&self) -> (f64, f64) {
        match (self.horizontal, self.vertical) {
            (Horizontal::Right, _) => (28.0, 0.0),
            (Horizontal::Left, _) => (-28.0, 0.0),
            (Horizontal::Center, Vertical::Top) => (0.0, -18.0),
            (Horizontal::Center, Vertical::Bottom) => (0.0, 18.0),
        }
    }

    /// The screen edges the window is anchored to.
    fn anchored(&self) -> Insets {
        Insets {
            top: i32::from(self.vertical == Vertical::Top),
            bottom: i32::from(self.vertical == Vertical::Bottom),
            left: i32::from(self.horizontal == Horizontal::Left),
            right: i32::from(self.horizontal == Horizontal::Right),
        }
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

/// Room around the cards inside the window: the shadow's reach, except
/// that towards an anchored screen edge there is no point reaching past
/// the screen, so the padding there is at most the margin.
pub fn padding(anchor: Anchor, shadow_reach: Insets, margin: i32) -> Insets {
    let anchored = anchor.anchored();
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
pub fn window_margins(anchor: Anchor, padding: Insets, margin: i32) -> Insets {
    let anchored = anchor.anchored();
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
    fn popups_slide_in_from_the_anchored_side() {
        let at = |vertical, horizontal| Anchor {
            vertical,
            horizontal,
        };
        assert_eq!(
            at(Vertical::Top, Horizontal::Right).enter_offset(),
            (28.0, 0.0)
        );
        assert_eq!(
            at(Vertical::Bottom, Horizontal::Left).enter_offset(),
            (-28.0, 0.0)
        );
        assert_eq!(
            at(Vertical::Top, Horizontal::Center).enter_offset(),
            (0.0, -18.0)
        );
        assert_eq!(
            at(Vertical::Bottom, Horizontal::Center).enter_offset(),
            (0.0, 18.0)
        );
    }

    #[test]
    fn top_right_cards_sit_exactly_margin_from_the_corner() {
        let anchor = Anchor::default();
        let pad = padding(anchor, DAWN_REACH, 16);
        let margins = window_margins(anchor, pad, 16);

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
