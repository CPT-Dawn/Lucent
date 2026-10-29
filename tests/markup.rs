//! Seam 1, body markup: whatever a sender puts in the body, the Popup gets
//! only the spec's `b`, `i`, `u` and `a`, as well-formed markup.

use lucent::core::{Core, Effect, Event, Notification};

/// The body the Popup shows for a Notification sent with `body`.
fn shown(body: &str) -> String {
    let mut core = Core::new();
    let effects = core.handle(Event::Notify(Notification {
        summary: "s".into(),
        body: body.into(),
        ..Default::default()
    }));
    effects
        .iter()
        .find_map(|e| match e {
            Effect::Stack(popups) => popups.first().map(|p| p.body.clone()),
            _ => None,
        })
        .expect("Notify should show a Popup")
}

#[test]
fn plain_text_is_shown_as_is() {
    assert_eq!(shown("World"), "World");
}

#[test]
fn bold_italic_underline_and_links_are_kept() {
    assert_eq!(
        shown(
            r#"<b>bold</b> <i>it</i> <u>under</u> <a href="https://x.dev/?a=1&amp;b=2">link</a>"#
        ),
        "<b>bold</b> <i>it</i> <u>under</u> <a>link</a>"
    );
}

#[test]
fn other_tags_are_removed_but_their_text_is_kept() {
    assert_eq!(
        shown(r#"<span color="red">red</span> <img src="x.png" alt="a > b"/><big>big</big>"#),
        "red big"
    );
}

#[test]
fn line_breaks_become_newlines() {
    assert_eq!(shown("one<br>two<BR/>three"), "one\ntwo\nthree");
}

#[test]
fn unbalanced_tags_are_closed_or_dropped() {
    assert_eq!(shown("<b>bold"), "<b>bold</b>");
    assert_eq!(shown("plain</i> text"), "plain text");
    assert_eq!(
        shown("<b>bold <i>both</b> italic?</i>"),
        "<b>bold <i>both</i></b> italic?"
    );
}

#[test]
fn stray_ampersands_are_escaped_and_real_entities_kept() {
    assert_eq!(
        shown("Tom & Jerry &amp; &lt;3 &#65;&#x42; &bogus; &#0; &"),
        "Tom &amp; Jerry &amp; &lt;3 &#65;&#x42; &amp;bogus; &amp;#0; &amp;"
    );
}

#[test]
fn angle_brackets_that_are_not_tags_are_escaped() {
    assert_eq!(shown("1 < 2 > 0 <3 <b"), "1 &lt; 2 &gt; 0 &lt;3 &lt;b");
}
