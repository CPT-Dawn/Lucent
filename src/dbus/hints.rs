//! Turns the raw `a{sv}` hints of `Notify` into typed [`Hints`], so the
//! engine never sees a D-Bus variant. Malformed hints are ignored rather
//! than failing the whole Notification.

use std::collections::HashMap;
use std::path::PathBuf;

use lucent::engine::{Hints, Image, ImageData, Urgency};
use zbus::{OwnedVariant, Variant};

pub fn parse(hints: &HashMap<String, OwnedVariant>) -> Hints {
    let get = |names: &[&str]| -> Option<&Variant<'_>> {
        names
            .iter()
            .find_map(|name| hints.get(*name))
            .map(|v| unwrap_variant(v))
    };

    let image_data = get(&["image-data", "image_data", "icon_data"]).and_then(image_data);
    let image_path = get(&["image-path", "image_path"])
        .and_then(string)
        .filter(|s| !s.is_empty())
        .map(|s| Image::Path(PathBuf::from(s)));

    Hints {
        urgency: get(&["urgency"])
            .and_then(integer)
            .and_then(|n| u8::try_from(n).ok())
            .map(Urgency::from_byte)
            .unwrap_or_default(),
        image: image_data.map(Image::Data).or(image_path),
        progress: get(&["value"])
            .and_then(integer)
            .map(|n| n.clamp(0, 100) as u8),
        transient: get(&["transient"]).and_then(boolean).unwrap_or(false),
        desktop_entry: get(&["desktop-entry"]).and_then(string),
        category: get(&["category"]).and_then(string),
    }
}

fn unwrap_variant<'a>(mut value: &'a Variant<'a>) -> &'a Variant<'a> {
    while let Variant::Variant(inner) = value {
        value = inner;
    }
    value
}

fn integer(value: &Variant<'_>) -> Option<i64> {
    match unwrap_variant(value) {
        Variant::U8(n) => Some(i64::from(*n)),
        Variant::I16(n) => Some(i64::from(*n)),
        Variant::U16(n) => Some(i64::from(*n)),
        Variant::I32(n) => Some(i64::from(*n)),
        Variant::U32(n) => Some(i64::from(*n)),
        Variant::I64(n) => Some(*n),
        Variant::U64(n) => i64::try_from(*n).ok(),
        _ => None,
    }
}

fn boolean(value: &Variant<'_>) -> Option<bool> {
    match unwrap_variant(value) {
        Variant::Bool(b) => Some(*b),
        other => integer(other).map(|n| n != 0),
    }
}

fn string(value: &Variant<'_>) -> Option<String> {
    match unwrap_variant(value) {
        Variant::Str(s) => Some(s.as_str().to_owned()),
        _ => None,
    }
}

/// `image-data` is `(iiibiiay)`: width, height, rowstride, has alpha,
/// bits per sample, channels, pixels.
fn image_data(value: &Variant<'_>) -> Option<ImageData> {
    let Variant::Structure(structure) = unwrap_variant(value) else {
        return None;
    };
    let [width, height, rowstride, has_alpha, bits, channels, data] = structure.fields() else {
        return None;
    };
    let int = |v: &Variant<'_>| integer(v).and_then(|n| i32::try_from(n).ok());
    let Variant::Array(bytes) = unwrap_variant(data) else {
        return None;
    };
    let data = bytes
        .inner()
        .iter()
        .map(|b| match b {
            Variant::U8(b) => Some(*b),
            _ => None,
        })
        .collect::<Option<Vec<u8>>>()?;
    Some(ImageData {
        width: int(width)?,
        height: int(height)?,
        rowstride: int(rowstride)?,
        has_alpha: boolean(has_alpha)?,
        bits_per_sample: int(bits)?,
        channels: int(channels)?,
        data: data.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hints(entries: Vec<(&str, Variant<'static>)>) -> HashMap<String, OwnedVariant> {
        entries
            .into_iter()
            .map(|(k, v)| (k.to_owned(), OwnedVariant::try_from(v).unwrap()))
            .collect()
    }

    #[test]
    fn no_hints_means_a_plain_normal_notification() {
        assert_eq!(parse(&HashMap::new()), Hints::default());
    }

    #[test]
    fn typed_hints_are_parsed() {
        let parsed = parse(&hints(vec![
            ("urgency", Variant::from(2u8)),
            ("value", Variant::from(140i32)),
            ("transient", Variant::from(true)),
            ("category", Variant::from("email.arrived")),
            ("desktop-entry", Variant::from("thunderbird")),
            ("image-path", Variant::from("/tmp/cover.png")),
        ]));

        assert_eq!(parsed.urgency, Urgency::Critical);
        assert_eq!(parsed.progress, Some(100));
        assert!(parsed.transient);
        assert_eq!(parsed.category.as_deref(), Some("email.arrived"));
        assert_eq!(parsed.desktop_entry.as_deref(), Some("thunderbird"));
        assert_eq!(parsed.image, Some(Image::Path("/tmp/cover.png".into())));
    }

    #[test]
    fn image_data_wins_over_image_path() {
        let pixels = Variant::from((1i32, 1i32, 4i32, true, 8i32, 4i32, vec![1u8, 2, 3, 4]));
        let parsed = parse(&hints(vec![
            ("image-path", Variant::from("/tmp/cover.png")),
            ("image-data", pixels),
        ]));

        let Some(Image::Data(data)) = parsed.image else {
            panic!("{:?}", parsed.image);
        };
        assert_eq!((data.width, data.channels), (1, 4));
        assert_eq!(&*data.data, &[1, 2, 3, 4]);
    }

    #[test]
    fn malformed_hints_are_ignored() {
        let parsed = parse(&hints(vec![
            ("urgency", Variant::from("very")),
            ("image-data", Variant::from(7i32)),
        ]));

        assert_eq!(parsed, Hints::default());
    }
}
