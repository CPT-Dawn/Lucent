//! The blur region a translucent Popup asks the compositor for: its rounded
//! shape, built from the rectangles a `wl_region` takes.

use lucent::wayland::rounded_region;

/// Whether (`x`, `y`) lies in any of `rects`.
fn covered(rects: &[(i32, i32, i32, i32)], x: i32, y: i32) -> bool {
    rects
        .iter()
        .any(|&(rx, ry, w, h)| (rx..rx + w).contains(&x) && (ry..ry + h).contains(&y))
}

#[test]
fn the_region_covers_the_middle_but_not_the_corners() {
    let rects = rounded_region(360, 100, 20);

    assert!(covered(&rects, 180, 50), "centre");
    assert!(covered(&rects, 0, 50), "left edge, mid-height");
    assert!(covered(&rects, 180, 0), "top edge, mid-width");
    for (x, y) in [(0, 0), (359, 0), (0, 99), (359, 99)] {
        assert!(!covered(&rects, x, y), "corner pixel ({x}, {y})");
    }
}

#[test]
fn the_region_stays_inside_the_surface() {
    for (w, h, r) in [(360, 100, 20), (10, 10, 50), (1, 1, 0), (0, 0, 20)] {
        for (x, y, rw, rh) in rounded_region(w, h, r) {
            assert!(x >= 0 && y >= 0 && rw >= 0 && rh >= 0);
            assert!(x + rw <= w as i32 && y + rh <= h as i32, "{w}x{h} r{r}");
        }
    }
}

#[test]
fn square_corners_are_one_rectangle() {
    assert_eq!(rounded_region(360, 100, 0), [(0, 0, 360, 100)]);
}
