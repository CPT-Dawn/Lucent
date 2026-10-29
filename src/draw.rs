//! Paints one Popup with Cairo and Pango (ADR-0001). Pure pixels: it knows
//! nothing about Wayland surfaces or buffers.
//!
//! The colours are the basic Midnight Bloom set; Themes and Presets replace
//! these constants in their own ticket.

use std::f64::consts::{FRAC_PI_2, PI};

use cairo::{Context, Format, ImageSurface, Operator};
use pango::{EllipsizeMode, FontDescription, Layout, WrapMode};

use crate::core::Popup;

const PADDING: f64 = 16.0;
const LINE_GAP: f64 = 4.0;
const RADIUS: f64 = 20.0;
const BODY_MAX_LINES: i32 = 6;
const FONT: &str = "Inter, sans-serif";
/// Action buttons: at most this many share a row; more wrap below.
const BUTTONS_PER_ROW: usize = 3;
const BUTTON_HEIGHT: f64 = 28.0;
const BUTTON_GAP: f64 = 8.0;
const BUTTON_RADIUS: f64 = 10.0;
const BUTTON_PADDING: f64 = 8.0;

type Rgba = (f64, f64, f64, f64);
const BACKGROUND: Rgba = rgba(0x14, 0x1B, 0x1F, 0.82);
const BORDER: Rgba = rgba(0xFC, 0xFD, 0xFD, 0.13);
const APP_NAME: Rgba = rgba(0x92, 0xA8, 0xA9, 1.0);
const SUMMARY: Rgba = rgba(0xC0, 0xCA, 0xF5, 1.0);
const BODY: Rgba = rgba(0xA9, 0xB1, 0xD6, 1.0);
const ACCENT: Rgba = rgba(0x8D, 0xE3, 0xDC, 1.0);
const BUTTON: Rgba = rgba(0xFC, 0xFD, 0xFD, 0.08);
/// Label colour on an accent-filled button: the background, made opaque.
const ON_ACCENT: Rgba = rgba(0x14, 0x1B, 0x1F, 1.0);

const fn rgba(r: u8, g: u8, b: u8, a: f64) -> Rgba {
    (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0, a)
}

/// The text blocks of a Popup, top to bottom, with their colours.
fn layouts(cr: &Context, popup: &Popup, width: u32) -> Vec<(Layout, Rgba)> {
    let text_width = ((width as f64 - 2.0 * PADDING) * pango::SCALE as f64) as i32;
    let block = |size: &str, max_lines: i32| {
        let layout = pangocairo::functions::create_layout(cr);
        layout.set_font_description(Some(&FontDescription::from_string(&format!(
            "{FONT} {size}"
        ))));
        layout.set_width(text_width);
        layout.set_wrap(WrapMode::WordChar);
        layout.set_ellipsize(EllipsizeMode::End);
        layout.set_height(-max_lines);
        layout
    };

    let mut blocks = Vec::with_capacity(4);
    if !popup.app_name.is_empty() {
        let layout = block("9", 1);
        layout.set_text(&popup.app_name);
        blocks.push((layout, APP_NAME));
    }
    let summary = block("Bold 11", 2);
    summary.set_text(&popup.summary);
    blocks.push((summary, SUMMARY));
    if !popup.body.is_empty() {
        let layout = block("10", BODY_MAX_LINES);
        // Invalid markup would make Pango drop the text; show it plain instead.
        // Proper markup cleaning arrives with its own ticket.
        if pango::parse_markup(&popup.body, '\0').is_ok() {
            layout.set_markup(&popup.body);
        } else {
            layout.set_text(&popup.body);
        }
        blocks.push((layout, BODY));
    }
    if popup.more > 0 {
        let layout = block("9", 1);
        layout.set_text(&format!("+{} more", popup.more));
        layout.set_alignment(pango::Alignment::Right);
        blocks.push((layout, APP_NAME));
    }
    blocks
}

fn content_height(blocks: &[(Layout, Rgba)]) -> f64 {
    let text: f64 = blocks
        .iter()
        .map(|(l, _)| l.pixel_extents().1.height() as f64)
        .sum();
    text + LINE_GAP * blocks.len().saturating_sub(1) as f64
}

/// A rectangle in surface pixels, from the Popup's top-left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    /// Whether (`x`, `y`) lies inside, counting the top and left edges.
    pub fn contains(&self, x: f64, y: f64) -> bool {
        (self.x..self.x + self.width).contains(&x) && (self.y..self.y + self.height).contains(&y)
    }
}

/// The height of the Popup's text blocks at `width`, laid out on a scratch
/// surface.
fn text_height(popup: &Popup, width: u32) -> f64 {
    ImageSurface::create(Format::ARgb32, 1, 1)
        .and_then(|s| Context::new(&s))
        .map(|cr| content_height(&layouts(&cr, popup, width)))
        .unwrap_or(0.0)
}

/// Lays out `count` buttons below text `text_height` tall: rows of up to
/// [`BUTTONS_PER_ROW`], each row's buttons sharing its width equally.
fn button_rects(count: usize, text_height: f64, width: u32) -> Vec<Rect> {
    let inner = width as f64 - 2.0 * PADDING;
    let top = PADDING + text_height + BUTTON_GAP;
    (0..count)
        .map(|i| {
            let row = i / BUTTONS_PER_ROW;
            let in_row = (count - row * BUTTONS_PER_ROW).min(BUTTONS_PER_ROW) as f64;
            let column = (i % BUTTONS_PER_ROW) as f64;
            let w = (inner - BUTTON_GAP * (in_row - 1.0)) / in_row;
            Rect {
                x: PADDING + column * (w + BUTTON_GAP),
                y: top + row as f64 * (BUTTON_HEIGHT + BUTTON_GAP),
                width: w,
                height: BUTTON_HEIGHT,
            }
        })
        .collect()
}

/// Where each of the Popup's Action buttons sits at `width`, in order.
pub fn buttons(popup: &Popup, width: u32) -> Vec<Rect> {
    if popup.actions.is_empty() {
        return Vec::new();
    }
    button_rects(popup.actions.len(), text_height(popup, width), width)
}

/// The height a Popup needs at `width`, in surface pixels.
pub fn measure(popup: &Popup, width: u32) -> u32 {
    let text = text_height(popup, width);
    let bottom = button_rects(popup.actions.len(), text, width)
        .last()
        .map_or(PADDING + text, |b| b.y + b.height);
    (bottom + PADDING).ceil() as u32
}

/// Paints the whole Popup over every pixel of `cr`'s `width`×`height` target,
/// with the Action button at index `hovered` in the accent colour.
pub fn paint(
    cr: &Context,
    popup: &Popup,
    width: u32,
    height: u32,
    hovered: Option<usize>,
) -> Result<(), cairo::Error> {
    let (w, h) = (width as f64, height as f64);

    // Buffers are reused, so start from fully transparent.
    cr.set_operator(Operator::Source);
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.0);
    cr.paint()?;
    cr.set_operator(Operator::Over);

    // Offset by half a pixel so the 1 px border lands on whole pixels.
    rounded_rect(cr, 0.5, 0.5, w - 1.0, h - 1.0, RADIUS);
    set_source(cr, BACKGROUND);
    cr.fill_preserve()?;
    set_source(cr, BORDER);
    cr.set_line_width(1.0);
    cr.stroke()?;

    let blocks = layouts(cr, popup, width);
    let mut y = PADDING;
    for (layout, colour) in &blocks {
        cr.move_to(PADDING, y);
        set_source(cr, *colour);
        pangocairo::functions::show_layout(cr, layout);
        y += layout.pixel_extents().1.height() as f64 + LINE_GAP;
    }

    let rects = button_rects(popup.actions.len(), content_height(&blocks), width);
    for (index, (action, rect)) in popup.actions.iter().zip(&rects).enumerate() {
        let (fill, text) = if hovered == Some(index) {
            (ACCENT, ON_ACCENT)
        } else {
            (BUTTON, SUMMARY)
        };
        rounded_rect(cr, rect.x, rect.y, rect.width, rect.height, BUTTON_RADIUS);
        set_source(cr, fill);
        cr.fill()?;

        // One line, centred, cut with an ellipsis when it does not fit.
        let label = pangocairo::functions::create_layout(cr);
        label.set_font_description(Some(&FontDescription::from_string(&format!(
            "{FONT} Medium 9"
        ))));
        label.set_width(((rect.width - 2.0 * BUTTON_PADDING) * pango::SCALE as f64) as i32);
        label.set_ellipsize(EllipsizeMode::End);
        label.set_single_paragraph_mode(true);
        label.set_alignment(pango::Alignment::Center);
        label.set_text(&action.label);
        let label_height = label.pixel_extents().1.height() as f64;
        cr.move_to(
            rect.x + BUTTON_PADDING,
            rect.y + (rect.height - label_height) / 2.0,
        );
        set_source(cr, text);
        pangocairo::functions::show_layout(cr, &label);
    }
    Ok(())
}

fn set_source(cr: &Context, (r, g, b, a): Rgba) {
    cr.set_source_rgba(r, g, b, a);
}

fn rounded_rect(cr: &Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -FRAC_PI_2, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, FRAC_PI_2);
    cr.arc(x + r, y + h - r, r, FRAC_PI_2, PI);
    cr.arc(x + r, y + r, r, PI, 3.0 * FRAC_PI_2);
    cr.close_path();
}
