//! Body markup. The freedesktop spec allows a small subset of HTML-ish
//! markup in bodies (`b`, `i`, `u`, `a href`, `img`). Apps get it wrong
//! often, so a body is checked before rendering: well-formed markup is
//! passed on as safe GTK label markup, and anything broken falls back to
//! the body as escaped plain text, so a message is never lost.

/// GTK label markup for a Notification body.
pub fn body_markup(body: &str) -> String {
    sanitize(body).unwrap_or_else(|| escape(body))
}

/// Escape text for use inside markup.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

/// Rebuild `body` from the tags Lucent understands, or `None` if it is
/// not well-formed.
fn sanitize(body: &str) -> Option<String> {
    let mut out = String::with_capacity(body.len());
    let mut open: Vec<&'static str> = Vec::new();
    let mut rest = body;
    while let Some(c) = rest.chars().next() {
        match c {
            '<' => {
                let end = rest.find('>')?;
                let tag = Tag::parse(&rest[1..end])?;
                rest = &rest[end + 1..];
                match tag {
                    Tag::Open(name, href) => {
                        open.push(name);
                        match href {
                            Some(href) => {
                                out.push_str("<a href=\"");
                                out.push_str(&escape(&href));
                                out.push_str("\">");
                            }
                            None => {
                                out.push('<');
                                out.push_str(name);
                                out.push('>');
                            }
                        }
                    }
                    Tag::Close(name) => {
                        if open.pop()? != name {
                            return None;
                        }
                        out.push_str("</");
                        out.push_str(name);
                        out.push('>');
                    }
                    Tag::LineBreak => out.push('\n'),
                    Tag::Image(alt) => out.push_str(&escape(&alt)),
                }
            }
            '&' => {
                let end = rest.find(';')?;
                out.push_str(&escape(&entity(&rest[1..end])?.to_string()));
                rest = &rest[end + 1..];
            }
            c => {
                out.push_str(&escape(c.encode_utf8(&mut [0; 4])));
                rest = &rest[c.len_utf8()..];
            }
        }
    }
    open.is_empty().then_some(out)
}

/// A tag Lucent understands.
enum Tag {
    /// `<b>`, `<i>`, `<u>`, or `<a>` with its href.
    Open(&'static str, Option<String>),
    Close(&'static str),
    /// `<br>` or `<br/>`.
    LineBreak,
    /// `<img>`: not drawn; its alt text stands in.
    Image(String),
}

impl Tag {
    fn parse(inner: &str) -> Option<Tag> {
        let inner = inner.trim();
        if let Some(name) = inner.strip_prefix('/') {
            return Some(Tag::Close(known(name.trim())?));
        }
        let self_closing = inner.ends_with('/');
        let inner = inner.trim_end_matches('/');
        let (name, attrs) = inner.split_once(char::is_whitespace).unwrap_or((inner, ""));
        let attrs = attributes(attrs)?;
        let attr = |key: &str| {
            attrs
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(key))
                .map(|(_, v)| v.clone())
        };
        match name.to_ascii_lowercase().as_str() {
            "br" => Some(Tag::LineBreak),
            "img" => Some(Tag::Image(attr("alt").unwrap_or_default())),
            _ if self_closing => None,
            "a" => Some(Tag::Open("a", Some(attr("href")?))),
            other => Some(Tag::Open(known(other)?, None)),
        }
    }
}

fn known(name: &str) -> Option<&'static str> {
    ["b", "i", "u", "a"]
        .into_iter()
        .find(|known| known.eq_ignore_ascii_case(name))
}

/// `key="value"` pairs (single or double quotes), with entities decoded.
fn attributes(mut text: &str) -> Option<Vec<(String, String)>> {
    let mut attrs = Vec::new();
    loop {
        text = text.trim_start();
        if text.is_empty() {
            return Some(attrs);
        }
        let (key, after) = text.split_once('=')?;
        let after = after.trim_start();
        let quote = after.chars().next().filter(|q| *q == '"' || *q == '\'')?;
        let close = after[1..].find(quote)? + 1;
        attrs.push((key.trim().to_owned(), decode(&after[1..close])?));
        text = &after[close + 1..];
    }
}

/// Decode the entities in an attribute value.
fn decode(text: &str) -> Option<String> {
    let mut out = String::new();
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let end = rest[amp..].find(';')? + amp;
        out.push(entity(&rest[amp + 1..end])?);
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    Some(out)
}

/// The character an entity name (between `&` and `;`) stands for.
fn entity(name: &str) -> Option<char> {
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => {
            let number = name.strip_prefix('#')?;
            let code = match number.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => number.parse().ok()?,
            };
            char::from_u32(code).filter(|c| *c != '\0')
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_passes_through() {
        assert_eq!(body_markup("See you at 5"), "See you at 5");
    }

    #[test]
    fn bold_italic_and_underline_are_kept() {
        assert_eq!(
            body_markup("<b>Build</b> <i>passed</i> <u>now</u>"),
            "<b>Build</b> <i>passed</i> <u>now</u>"
        );
    }

    #[test]
    fn links_keep_only_their_href() {
        assert_eq!(
            body_markup(r#"Read <a href="https://x.dev/?a=1&amp;b=2" target="_blank">this</a>"#),
            r#"Read <a href="https://x.dev/?a=1&amp;b=2">this</a>"#
        );
    }

    #[test]
    fn entities_are_kept() {
        assert_eq!(
            body_markup("Tom &amp; Jerry &#9829;"),
            "Tom &amp; Jerry \u{2665}"
        );
    }

    #[test]
    fn line_breaks_become_newlines_and_images_their_alt_text() {
        assert_eq!(
            body_markup(r#"one<br/>two<br>three <img src="x.png" alt="[pic]"/>"#),
            "one\ntwo\nthree [pic]"
        );
    }

    #[test]
    fn broken_markup_falls_back_to_escaped_plain_text() {
        for body in [
            "<b>unclosed",
            "<b><i>crossed</b></i>",
            "a < b",
            "Tom & Jerry",
            "<blink>hi</blink>",
            "</b>",
            "&bogus;",
            r#"<a>no href</a>"#,
        ] {
            assert_eq!(body_markup(body), escape(body), "{body}");
        }
    }

    #[test]
    fn escaped_plain_text_reads_like_the_original() {
        assert_eq!(escape("a < b & \"c\""), "a &lt; b &amp; &quot;c&quot;");
    }
}
