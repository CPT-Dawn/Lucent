//! Body markup cleaning: reduces whatever a sender put in a body to the
//! subset the freedesktop spec allows (`b`, `i`, `u`, `a`), as well-formed
//! markup, so a misbehaving app can never break a Popup.
//!
//! Links keep their text but lose every attribute: they are styled, never
//! clickable, so the target is not needed.

/// Tags the spec allows in a body.
const ALLOWED: [&str; 4] = ["b", "i", "u", "a"];

/// Cleans `body` down to the allowed tags.
pub fn clean(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    // Allowed tags currently open, innermost last.
    let mut open: Vec<&'static str> = Vec::new();
    let mut rest = body;
    while let Some(start) = rest.find('<') {
        push_text(&mut out, &rest[..start]);
        rest = &rest[start..];
        let Some(tag) = Tag::parse(rest) else {
            out.push_str("&lt;");
            rest = &rest[1..];
            continue;
        };
        rest = &rest[tag.len..];
        if tag.name == "br" {
            out.push('\n');
            continue;
        }
        let Some(name) = ALLOWED.into_iter().find(|a| *a == tag.name) else {
            continue;
        };
        // `<b/>` styles nothing; it must not open a tag for the rest.
        if tag.empty {
            continue;
        }
        if !tag.closing {
            open.push(name);
            push_tag(&mut out, name, false);
        } else if let Some(depth) = open.iter().rposition(|o| *o == name) {
            // Close whatever was opened inside it too; those tags are
            // dropped from here on rather than reopened.
            for inner in open.drain(depth..).rev() {
                push_tag(&mut out, inner, true);
            }
        }
    }
    push_text(&mut out, rest);
    for name in open.into_iter().rev() {
        push_tag(&mut out, name, true);
    }
    out
}

/// Appends text, escaping every `&` that does not start a valid entity and
/// every `>`. (A `<` never reaches here: it is either a tag or escaped.)
fn push_text(out: &mut String, text: &str) {
    for (i, c) in text.char_indices() {
        match c {
            '&' if is_entity(&text[i..]) => out.push('&'),
            '&' => out.push_str("&amp;"),
            '>' => out.push_str("&gt;"),
            c => out.push(c),
        }
    }
}

/// Whether `text` (which begins with `&`) starts with an entity Pango
/// accepts.
fn is_entity(text: &str) -> bool {
    let Some(end) = text.find(';') else {
        return false;
    };
    let name = &text[1..end];
    let Some(num) = name.strip_prefix('#') else {
        return ["amp", "lt", "gt", "quot", "apos"].contains(&name);
    };
    let code = match num.strip_prefix(['x', 'X']) {
        Some(hex) if hex.bytes().all(|b| b.is_ascii_hexdigit()) => u32::from_str_radix(hex, 16),
        Some(_) => return false,
        None if num.bytes().all(|b| b.is_ascii_digit()) => num.parse(),
        None => return false,
    };
    // Only the characters XML allows: GMarkup, and so Pango, rejects the
    // rest, and the whole body with them.
    code.ok()
        .filter(|&c| {
            matches!(c, 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF)
        })
        .is_some()
}

fn push_tag(out: &mut String, name: &str, closing: bool) {
    out.push('<');
    if closing {
        out.push('/');
    }
    out.push_str(name);
    out.push('>');
}

/// One `<…>` tag at the start of some text.
struct Tag {
    /// Lowercased.
    name: String,
    closing: bool,
    /// Written `<name/>`, so it holds nothing.
    empty: bool,
    /// Bytes from `<` to `>` inclusive.
    len: usize,
}

impl Tag {
    /// Reads a tag from the start of `text`, which begins with `<`. `None`
    /// when it is not a tag (no name, or never closed by `>`).
    fn parse(text: &str) -> Option<Tag> {
        let inner = &text[1..];
        let (closing, inner) = match inner.strip_prefix('/') {
            Some(after) => (true, after),
            None => (false, inner),
        };
        let name_len = inner
            .find(|c: char| !c.is_ascii_alphanumeric())
            .unwrap_or(inner.len());
        if name_len == 0 || !inner.starts_with(|c: char| c.is_ascii_alphabetic()) {
            return None;
        }
        // Find the closing `>`, skipping any inside quoted attribute values.
        let mut quote = None;
        let end = inner[name_len..].char_indices().find_map(|(i, c)| {
            match (quote, c) {
                (None, '"' | '\'') => quote = Some(c),
                (Some(q), c) if c == q => quote = None,
                (None, '>') => return Some(i),
                _ => {}
            }
            None
        })?;
        Some(Tag {
            name: inner[..name_len].to_ascii_lowercase(),
            closing,
            empty: inner[name_len..name_len + end].trim_end().ends_with('/'),
            len: text.len() - inner.len() + name_len + end + 1,
        })
    }
}
