//! A Notification's image, free of any transport types.
//!
//! The D-Bus adapter picks which hint to use and unpacks it; this module
//! turns what it finds into an [`Image`]. Inline pixels are converted and
//! scaled down here, once, as they arrive, so a screenshot sent as
//! `image-data` never stays in memory at full size. Files and icon names are
//! only named here; [`crate::icon`] loads them.

use std::path::PathBuf;
use std::sync::Arc;

/// The longest side pixel hints are kept at. Popups draw icons far smaller,
/// so this leaves room for any icon size without holding megabytes.
pub const MAX_SIDE: u32 = 128;

/// A Notification's image, before it is loaded for drawing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Image {
    /// Pixels sent inline, already scaled down.
    Pixels(Arc<Pixels>),
    /// An image file named by an absolute path or a `file://` URI.
    File(PathBuf),
    /// A name to look up in the icon theme.
    Icon(String),
}

/// Premultiplied, native-endian ARGB32 pixels with no row padding: the
/// layout of Cairo's `ARgb32` at a stride of `4 * width`.
#[derive(Clone, PartialEq, Eq)]
pub struct Pixels {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

impl std::fmt::Debug for Pixels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
}

impl Pixels {
    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }
}

impl Image {
    /// Reads an icon name, absolute path or `file://` URI; empty is none.
    pub fn from_name(s: &str) -> Option<Self> {
        if s.is_empty() {
            None
        } else if let Some(path) = s.strip_prefix("file://") {
            Some(Self::File(percent_decode(path).into()))
        } else if s.starts_with('/') {
            Some(Self::File(s.into()))
        } else {
            Some(Self::Icon(s.to_owned()))
        }
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match (bytes[i], hex) {
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 3;
            }
            (byte, _) => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Pixels as a sender describes them: the fields of the spec's `(iiibiiay)`
/// image hints, in that order.
#[derive(Debug, Clone, Copy)]
pub struct RawPixels<'a> {
    pub width: i64,
    pub height: i64,
    pub rowstride: i64,
    pub has_alpha: bool,
    pub bits_per_sample: i64,
    pub channels: i64,
    pub data: &'a [u8],
}

impl Pixels {
    /// Converts and scales down `raw`, or `None` if it does not describe
    /// valid pixels. Only 8-bit RGB and RGBA exist in practice.
    pub fn from_raw(raw: RawPixels<'_>) -> Option<Self> {
        let RawPixels {
            width,
            height,
            rowstride,
            has_alpha,
            bits_per_sample: bits,
            channels,
            data,
        } = raw;

        let (width, height, rowstride, channels) = (
            usize::try_from(width).ok()?,
            usize::try_from(height).ok()?,
            usize::try_from(rowstride).ok()?,
            usize::try_from(channels).ok()?,
        );
        let valid = width > 0
            && height > 0
            && bits == 8
            && channels == if has_alpha { 4 } else { 3 }
            && rowstride >= width * channels
            && data.len() >= rowstride * (height - 1) + width * channels;
        if !valid {
            return None;
        }

        // Box-filter down so the longest side fits MAX_SIDE, averaging in
        // premultiplied space so transparent pixels do not darken edges.
        let longest = width.max(height);
        let max = MAX_SIDE as usize;
        let (out_w, out_h) = if longest > max {
            (
                (width * max / longest).max(1),
                (height * max / longest).max(1),
            )
        } else {
            (width, height)
        };
        let mut out = Vec::with_capacity(out_w * out_h * 4);
        for oy in 0..out_h {
            let (y0, y1) = (
                oy * height / out_h,
                ((oy + 1) * height / out_h).max(oy * height / out_h + 1),
            );
            for ox in 0..out_w {
                let (x0, x1) = (
                    ox * width / out_w,
                    ((ox + 1) * width / out_w).max(ox * width / out_w + 1),
                );
                let mut sum = [0u32; 4];
                for y in y0..y1 {
                    for x in x0..x1 {
                        let p = &data[y * rowstride + x * channels..][..channels];
                        let a = if has_alpha { u32::from(p[3]) } else { 255 };
                        sum[0] += a;
                        for c in 0..3 {
                            sum[c + 1] += (u32::from(p[c]) * a + 127) / 255;
                        }
                    }
                }
                let n = ((y1 - y0) * (x1 - x0)) as u32;
                let [a, r, g, b] = sum.map(|s| (s + n / 2) / n);
                out.extend_from_slice(&(a << 24 | r << 16 | g << 8 | b).to_ne_bytes());
            }
        }
        Some(Pixels {
            width: out_w as u32,
            height: out_h as u32,
            data: out,
        })
    }
}
