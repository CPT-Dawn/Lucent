//! The progress ring's state and geometry: a rounded-square ring traced
//! around the icon tile, and the "· N%" that replaces the time in the
//! header while a Notification carries the `value` hint.

use std::f64::consts::PI;

use super::motion::{RING, Tween};

/// What the header shows instead of the time: `· 64%`.
pub fn header_text(percent: u8) -> String {
    format!("· {percent}%")
}

/// Where the ring is drawn inside its square box, in pixels: a rounded
/// square stroked down its centre line, starting just right of the
/// top-left corner and running clockwise.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geometry {
    /// Distance from the box edge to the stroke's centre line.
    pub inset: f64,
    /// Side of the centre-line square.
    pub side: f64,
    /// Corner radius of the centre-line square.
    pub radius: f64,
    pub stroke: f64,
}

impl Geometry {
    /// The design's ring for a 52px box (a 42px tile plus 5px all round),
    /// scaled to `box_side`.
    pub fn for_box(box_side: i32) -> Self {
        let scale = f64::from(box_side) / 52.0;
        let stroke = 3.0 * scale;
        Self {
            inset: stroke / 2.0,
            side: f64::from(box_side) - stroke,
            radius: 15.0 * scale,
            stroke,
        }
    }

    /// Length of the centre line all the way round.
    pub fn perimeter(&self) -> f64 {
        4.0 * (self.side - 2.0 * self.radius) + 2.0 * PI * self.radius
    }

    /// The `(on, off)` dash that strokes `fraction` of the ring, or `None`
    /// when there is nothing to draw.
    pub fn dash(&self, fraction: f64) -> Option<(f64, f64)> {
        let fraction = fraction.clamp(0.0, 1.0);
        let length = self.perimeter();
        (fraction > 0.0).then_some((fraction * length, length))
    }
}

/// The value the ring shows, gliding to each new value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ring {
    tween: Tween,
}

impl Ring {
    /// A ring already showing `percent`.
    pub fn new(percent: u8) -> Self {
        Self {
            tween: Tween::settled(fraction(percent)),
        }
    }

    /// Glide to `percent` from wherever the ring is at `now_us`.
    pub fn set(&mut self, percent: u8, now_us: i64, reduced: bool) {
        self.tween = self
            .tween
            .retarget(fraction(percent), now_us, RING, reduced);
    }

    /// The share of the ring filled at `now_us`, `0..=1`.
    pub fn fraction(&self, now_us: i64) -> f64 {
        self.tween.value(now_us)
    }

    /// The percentage to show at `now_us`, counting along with the ring.
    pub fn percent(&self, now_us: i64) -> u8 {
        (self.fraction(now_us) * 100.0).round().clamp(0.0, 100.0) as u8
    }

    pub fn animating(&self, now_us: i64) -> bool {
        !self.tween.done(now_us)
    }
}

fn fraction(percent: u8) -> f64 {
    f64::from(percent.min(100)) / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: i64 = 1_000;

    #[test]
    fn the_header_shows_the_percentage_after_a_dot() {
        assert_eq!(header_text(64), "· 64%");
        assert_eq!(header_text(0), "· 0%");
        assert_eq!(header_text(100), "· 100%");
    }

    #[test]
    fn the_ring_is_a_rounded_square_one_stroke_inside_its_box() {
        // The design: a 49x49 rect at 1.5 with rx 15, stroke 3, in a 52px box.
        let ring = Geometry::for_box(52);
        assert_eq!(ring.inset, 1.5);
        assert_eq!(ring.side, 49.0);
        assert_eq!(ring.radius, 15.0);
        assert_eq!(ring.stroke, 3.0);
        // 4 straight runs of 19px plus a full circle of radius 15.
        assert!((ring.perimeter() - 170.2478).abs() < 1e-3);
    }

    #[test]
    fn the_filled_stroke_covers_the_share_of_the_perimeter_done() {
        let ring = Geometry::for_box(52);
        let length = ring.perimeter();

        assert_eq!(ring.dash(0.0), None, "nothing to draw at 0%");
        let (on, off) = ring.dash(0.5).unwrap();
        assert!((on - length / 2.0).abs() < 1e-9);
        assert!(off >= length, "the dash must not repeat");
        assert_eq!(ring.dash(1.7).unwrap().0, length);
    }

    #[test]
    fn a_new_ring_shows_its_value_at_once() {
        let ring = Ring::new(40);

        assert_eq!(ring.fraction(0), 0.4);
        assert_eq!(ring.percent(0), 40);
        assert!(!ring.animating(0));
    }

    #[test]
    fn a_new_value_glides_there_over_350ms() {
        let mut ring = Ring::new(20);
        ring.set(80, 1_000 * MS, false);

        let start = ring.fraction(1_000 * MS);
        let midway = ring.fraction(1_175 * MS);
        let end = ring.fraction(1_350 * MS);

        assert_eq!(start, 0.2);
        assert!(midway > 0.5 && midway < 0.8, "{midway}");
        assert!(ring.animating(1_349 * MS));
        assert_eq!(end, 0.8);
        assert_eq!(ring.percent(1_350 * MS), 80);
        assert!(!ring.animating(1_350 * MS));
    }

    #[test]
    fn the_percentage_counts_along_with_the_ring() {
        let mut ring = Ring::new(0);
        ring.set(100, 0, false);

        let shown = ring.percent(100 * MS);

        assert!(shown > 0 && shown < 100, "{shown}");
    }

    #[test]
    fn a_value_arriving_mid_glide_carries_on_from_where_the_ring_is() {
        let mut ring = Ring::new(0);
        ring.set(100, 0, false);
        let now = ring.fraction(100 * MS);

        ring.set(50, 100 * MS, false);

        assert_eq!(ring.fraction(100 * MS), now);
        assert_eq!(ring.fraction(450 * MS), 0.5);
    }

    #[test]
    fn with_reduced_motion_the_ring_jumps() {
        let mut ring = Ring::new(10);
        ring.set(90, 0, true);

        assert_eq!(ring.fraction(MS), 0.9);
        assert!(!ring.animating(MS));
    }
}
