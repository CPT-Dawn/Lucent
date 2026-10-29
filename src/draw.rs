//! Paints one Popup with Cairo and Pango (ADR-0001). Pure pixels: it knows
//! nothing about Wayland surfaces or buffers.
//! Every colour, size and font comes from the Popup's resolved Theme.

use std::f64::consts::{FRAC_PI_2, PI};

use cairo::{Context, FontOptions, Format, HintMetrics, ImageSurface, LinearGradient, Operator};
use pango::{EllipsizeMode, FontDescription, Layout, WrapMode};

use crate::core::Popup;
use crate::icon;
use crate::theme::Rgba;

const LINE_GAP: f64 = 4.0;
/// Thickness of the progress bar, in pixels.
const BAR_HEIGHT: f64 = 6.0;
/// Space between the icon and the text beside it.
const ICON_GAP: f64 = 12.0;

/// Where the text starts: beside the icon if there is one.
fn text_left(popup: &Popup, icon: Option<&ImageSurface>) -> f64 {
    let padding = f64::from(popup.theme.padding);
    icon.map_or(padding, |icon| padding + icon::extent(icon).0 + ICON_GAP)
}

/// A Pango context for `cr` whose glyph advances are not rounded to whole
/// device pixels, so text lays out the same at every scale: a Popup measured
/// before its monitor's scale is known still fits once drawn at it.
fn text_context(cr: &Context) -> pango::Context {
    let context = pangocairo::functions::create_context(cr);
    if let Ok(mut options) = FontOptions::new() {
        options.set_hint_metrics(HintMetrics::Off);
        pangocairo::functions::context_set_font_options(&context, Some(&options));
    }
    context
}
/// Actions: at most this many share a row; more wrap below.
const ACTIONS_PER_ROW: usize = 3;
const ACTION_HEIGHT: f64 = 28.0;
const ACTION_GAP: f64 = 8.0;
const ACTION_RADIUS: f64 = 10.0;
const ACTION_PADDING: f64 = 8.0;
/// How strongly a resting Action is filled, in percent of the summary colour.
const ACTION_WASH: u8 = 10;

/// The text blocks of a Popup, top to bottom, with their colours.
fn layouts(cr: &Context, popup: &Popup, left: f64, width: u32) -> Vec<(Layout, Rgba)> {
    let theme = &popup.theme;
    let padding = f64::from(theme.padding);
    let text_width = ((width as f64 - left - padding).max(0.0) * pango::SCALE as f64) as i32;
    let context = text_context(cr);
    let block = |style: &str, size: u32, max_lines: i32| {
        let layout = Layout::new(&context);
        layout.set_font_description(Some(&FontDescription::from_string(&format!(
            "{} {style} {size}",
            theme.font
        ))));
        layout.set_width(text_width);
        layout.set_wrap(WrapMode::WordChar);
        layout.set_ellipsize(EllipsizeMode::End);
        layout.set_height(-max_lines);
        layout
    };

    let mut blocks = Vec::with_capacity(4);
    if !popup.app_name.is_empty() {
        let layout = block("", theme.app_name_size, 1);
        layout.set_text(&popup.app_name);
        blocks.push((layout, theme.app_name));
    }
    let summary = block("Semi-Bold", theme.summary_size, 2);
    summary.set_text(&popup.summary);
    blocks.push((summary, theme.summary));
    if !popup.body.is_empty() {
        let lines = i32::try_from(popup.body_lines.max(1)).unwrap_or(i32::MAX);
        let layout = block("", theme.body_size, lines);
        set_body(&layout, &popup.body, theme.accent);
        blocks.push((layout, theme.body));
    }
    if popup.more > 0 {
        let layout = block("", theme.app_name_size, 1);
        layout.set_text(&format!("+{} more", popup.more));
        layout.set_alignment(pango::Alignment::Right);
        blocks.push((layout, theme.app_name));
    }
    blocks
}

/// Sets a body the core has already cleaned to `b`, `i`, `u` and `a`.
/// Links take the Theme's `accent` colour: styled, never clickable.
fn set_body(layout: &Layout, body: &str, link: Rgba) {
    let link_span = format!(
        r##"<span foreground="#{:02X}{:02X}{:02X}" fgalpha="{}" underline="single">"##,
        link.r,
        link.g,
        link.b,
        // Pango's alpha runs 1..=65535; 0 would mean "unset".
        u32::from(link.a).max(1) * 257
    );
    // Pango has no `<a>`; the cleaned body holds it only as these exact tags.
    let markup = body.replace("<a>", &link_span).replace("</a>", "</span>");
    // Cleaned markup always parses; should it ever not, Pango would drop
    // the text, so show it plain rather than lose it.
    if pango::parse_markup(&markup, '\0').is_ok() {
        layout.set_markup(&markup);
    } else {
        layout.set_text(body);
    }
}

fn content_height(blocks: &[(Layout, Rgba)]) -> f64 {
    let text: f64 = blocks
        .iter()
        .map(|(l, _)| l.pixel_extents().1.height() as f64)
        .sum();
    text + LINE_GAP * blocks.len().saturating_sub(1) as f64
}

/// The room a Popup's progress bar takes, gap included; none without one.
fn bar_room(popup: &Popup) -> f64 {
    if popup.progress.is_some() {
        BAR_HEIGHT + LINE_GAP
    } else {
        0.0
    }
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

/// The height of the Popup's body: its text and progress bar, or its icon
/// beside them if that is taller.
fn body_height(text: &[(Layout, Rgba)], popup: &Popup, icon: Option<&ImageSurface>) -> f64 {
    let text = content_height(text) + bar_room(popup);
    text.max(icon.map_or(0.0, |icon| icon::extent(icon).1))
}

/// The height of the Popup's body at `width`, laid out on a scratch surface.
fn measure_body(popup: &Popup, icon: Option<&ImageSurface>, width: u32) -> f64 {
    ImageSurface::create(Format::ARgb32, 1, 1)
        .and_then(|s| Context::new(&s))
        .map(|cr| {
            let blocks = layouts(&cr, popup, text_left(popup, icon), width);
            body_height(&blocks, popup, icon)
        })
        .unwrap_or(0.0)
}

/// Lays out `count` Actions below a body `body_height` tall, inset by
/// `padding`: rows of up to [`ACTIONS_PER_ROW`], each row's Actions sharing
/// the full inner width equally, under the icon as well as the text.
fn layout_actions(count: usize, body_height: f64, width: u32, padding: f64) -> Vec<Rect> {
    let inner = (width as f64 - 2.0 * padding).max(0.0);
    let top = padding + body_height + ACTION_GAP;
    (0..count)
        .map(|i| {
            let row = i / ACTIONS_PER_ROW;
            let in_row = (count - row * ACTIONS_PER_ROW).min(ACTIONS_PER_ROW) as f64;
            let column = (i % ACTIONS_PER_ROW) as f64;
            let w = ((inner - ACTION_GAP * (in_row - 1.0)) / in_row).max(0.0);
            Rect {
                x: padding + column * (w + ACTION_GAP),
                y: top + row as f64 * (ACTION_HEIGHT + ACTION_GAP),
                width: w,
                height: ACTION_HEIGHT,
            }
        })
        .collect()
}

/// Where each of the Popup's Actions sits at `width`, in order.
pub fn action_rects(popup: &Popup, icon: Option<&ImageSurface>, width: u32) -> Vec<Rect> {
    if popup.actions.is_empty() {
        return Vec::new();
    }
    let padding = f64::from(popup.theme.padding);
    layout_actions(
        popup.actions.len(),
        measure_body(popup, icon, width),
        width,
        padding,
    )
}

/// The height a Popup with `icon` needs at `width`, in surface pixels.
pub fn measure(popup: &Popup, icon: Option<&ImageSurface>, width: u32) -> u32 {
    let padding = f64::from(popup.theme.padding);
    let body = measure_body(popup, icon, width);
    let bottom = layout_actions(popup.actions.len(), body, width, padding)
        .last()
        .map_or(padding + body, |b| b.y + b.height);
    (bottom + padding).ceil() as u32
}

/// Paints the whole Popup over every pixel of `cr`'s `width`×`height` target,
/// with the Action at index `hovered` in the accent colour.
pub fn paint(
    cr: &Context,
    popup: &Popup,
    icon: Option<&ImageSurface>,
    width: u32,
    height: u32,
    hovered: Option<usize>,
) -> Result<(), cairo::Error> {
    let (w, h) = (width as f64, height as f64);
    let theme = &popup.theme;
    let padding = f64::from(theme.padding);

    // Buffers are reused, so start from fully transparent.
    cr.set_operator(Operator::Source);
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.0);
    cr.paint()?;
    cr.set_operator(Operator::Over);

    // Inset by half the border so it lands inside the surface, on whole
    // pixels when it is 1 px wide.
    let line = f64::from(theme.border_width);
    let inset = line / 2.0;
    let radius = f64::from(theme.radius)
        .min((w.min(h) - line) / 2.0)
        .max(0.0);
    rounded_rect(cr, inset, inset, w - line, h - line, radius);
    set_source(cr, theme.background);
    if line > 0.0 {
        cr.fill_preserve()?;
        // The top edge catches the light, fading into the rest by the
        // time the corners have turned.
        let edge = LinearGradient::new(0.0, 0.0, 0.0, radius.max(1.0));
        add_stop(&edge, 0.0, theme.border_top);
        add_stop(&edge, 1.0, theme.border);
        cr.set_source(&edge)?;
        cr.set_line_width(line);
        cr.stroke()?;
    } else {
        cr.fill()?;
    }

    if theme.accent_width > 0 && h > 2.0 * padding {
        let aw = f64::from(theme.accent_width);
        let x = ((padding - aw) / 2.0).max(line);
        rounded_rect(cr, x, padding, aw, h - 2.0 * padding, aw / 2.0);
        set_source(cr, theme.accent);
        cr.fill()?;
    }

    if let Some(icon) = icon {
        cr.set_source_surface(icon, padding, padding)?;
        cr.paint()?;
    }

    let left = text_left(popup, icon);
    let blocks = layouts(cr, popup, left, width);
    // The bar sits under the text, above any "+N more" hint.
    let bar_at = blocks.len() - usize::from(popup.more > 0);
    let mut y = padding;
    for (i, (layout, colour)) in blocks.iter().enumerate() {
        if i == bar_at {
            y += progress_bar(cr, popup, left, y, w)?;
        }
        cr.move_to(left, y);
        set_source(cr, *colour);
        pangocairo::functions::show_layout(cr, layout);
        y += layout.pixel_extents().1.height() as f64 + LINE_GAP;
    }
    if bar_at == blocks.len() {
        progress_bar(cr, popup, left, y, w)?;
    }

    let body = body_height(&blocks, popup, icon);
    let rects = layout_actions(popup.actions.len(), body, width, padding);
    let action_radius = ACTION_RADIUS.min(f64::from(theme.radius));
    let labels = text_context(cr);
    for (index, (action, rect)) in popup.actions.iter().zip(&rects).enumerate() {
        // Resting Actions are a faint wash of the summary colour, so they
        // suit light and dark Themes alike; the hovered one takes the
        // accent, its label the background made opaque.
        let (fill, text) = if hovered == Some(index) {
            (
                theme.accent,
                Rgba {
                    a: 255,
                    ..theme.background
                },
            )
        } else {
            (theme.summary.faded(ACTION_WASH), theme.summary)
        };
        rounded_rect(cr, rect.x, rect.y, rect.width, rect.height, action_radius);
        set_source(cr, fill);
        cr.fill()?;

        // One line, centred, cut with an ellipsis when it does not fit.
        let label = Layout::new(&labels);
        label.set_font_description(Some(&FontDescription::from_string(&format!(
            "{} Medium {}",
            theme.font, theme.app_name_size
        ))));
        let label_width = (rect.width - 2.0 * ACTION_PADDING).max(0.0);
        label.set_width((label_width * pango::SCALE as f64) as i32);
        label.set_ellipsize(EllipsizeMode::End);
        label.set_single_paragraph_mode(true);
        label.set_alignment(pango::Alignment::Center);
        label.set_text(&action.label);
        let label_height = label.pixel_extents().1.height() as f64;
        cr.move_to(
            rect.x + ACTION_PADDING,
            rect.y + (rect.height - label_height) / 2.0,
        );
        set_source(cr, text);
        pangocairo::functions::show_layout(cr, &label);
    }
    Ok(())
}

/// Paints the Popup's progress bar, if it has one, as a pill-shaped track
/// at `y` across the text column from `left`, filled in from the left;
/// returns the room it took.
fn progress_bar(
    cr: &Context,
    popup: &Popup,
    left: f64,
    y: f64,
    w: f64,
) -> Result<f64, cairo::Error> {
    let Some(progress) = popup.progress else {
        return Ok(0.0);
    };
    let theme = &popup.theme;
    let padding = f64::from(theme.padding);
    let width = (w - left - padding).max(0.0);
    cr.save()?;
    rounded_rect(cr, left, y, width, BAR_HEIGHT, BAR_HEIGHT / 2.0);
    set_source(cr, theme.progress_track);
    cr.fill_preserve()?;
    // Clip to the pill so even a sliver of fill keeps its rounded end.
    cr.clip();
    cr.rectangle(left, y, width * f64::from(progress) / 100.0, BAR_HEIGHT);
    set_source(cr, theme.progress_fill);
    cr.fill()?;
    cr.restore()?;
    Ok(bar_room(popup))
}

fn set_source(cr: &Context, colour: Rgba) {
    let (r, g, b, a) = colour.to_f64();
    cr.set_source_rgba(r, g, b, a);
}

fn add_stop(gradient: &LinearGradient, offset: f64, colour: Rgba) {
    let (r, g, b, a) = colour.to_f64();
    gradient.add_color_stop_rgba(offset, r, g, b, a);
}

fn rounded_rect(cr: &Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -FRAC_PI_2, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, FRAC_PI_2);
    cr.arc(x + r, y + h - r, r, FRAC_PI_2, PI);
    cr.arc(x + r, y + r, r, PI, 3.0 * FRAC_PI_2);
    cr.close_path();
}
