//! Lossless `media`/`mediaSingle`/`mediaGroup` <-> Markdown token codec.
//!
//! `to_markdown`/`compile` round-trip a description through `$EDITOR` as
//! plain Markdown, but Markdown's `![alt](url)` syntax has nowhere near
//! enough structure to carry a real ADF media node's attrs (`id`,
//! `collection`, `width`, `height`, `localId`, plus the wrapping
//! `mediaSingle`'s own `layout`/`width`/`widthType`). Rather than lose that
//! data every time a description gets edited, this module packs it into a
//! synthetic `adf-media://` URI sitting in the `(url)` slot of ordinary
//! image syntax — a scheme no real image URL a human types will ever
//! collide with — so `compile()` can reconstruct the exact original node.
//!
//! Token shapes:
//!   `![<alt>](adf-media://file/<id>?collection=<enc>&width=<w>&height=<h>&alt=<enc>&localId=<enc>)`
//!   `![<alt>](adf-media://external?url=<enc>&alt=<enc>&width=<w>&height=<h>)`
//! plus, when the media node is `mediaSingle`'s sole child:
//!   `&wrapper=single&layout=<enc>&msWidth=<w>&widthType=<enc>`
//! or, when it's one child of a `mediaGroup`:
//!   `&wrapper=group`
//! A bare top-level `media` node (no wrapper) carries neither param.
//!
//! Only attrs actually present on the source node are encoded — an absent
//! attr stays absent after a round trip, while a present-but-empty one
//! (e.g. `collection: ""`, common for issue attachments) round-trips as
//! an empty string rather than disappearing.
//!
//! The bracket's `<alt>` is a human-readable echo for anyone reading the
//! raw Markdown in `$EDITOR`; the `alt=` query param is the value actually
//! restored on decode, so alt text survives characters (`&`, unicode, …)
//! that would otherwise need escaping inside the bracket itself.
//!
//! Percent-encoding is hand-rolled here rather than reusing
//! `jira::live::support::url_encode`: that helper lives behind the `live`
//! feature, while `src/adf` has no feature gates and must build under
//! every feature set (see `cargo build --no-default-features`).

use serde_json::{json, Map, Value};

const SCHEME_FILE: &str = "adf-media://file/";
const SCHEME_EXTERNAL: &str = "adf-media://external";

/// How a `media` node was nested when it was encoded, so `decode` knows
/// what wrapper (if any) to reconstruct around it.
pub(super) enum Wrapper<'a> {
    /// A bare top-level `media` node, not wrapped in `mediaSingle`.
    None,
    /// `mediaSingle`'s sole child; carries `mediaSingle`'s own attrs.
    Single(&'a Value),
    /// One child of a `mediaGroup`.
    Group,
}

/// The reconstructed wrapper for a decoded token, mirroring [`Wrapper`].
pub(super) enum DecodedWrapper {
    None,
    Single(Value),
    Group,
}

pub(super) struct Decoded {
    pub media_attrs: Value,
    pub wrapper: DecodedWrapper,
}

/// True if `url` is one of this module's own tokens rather than a real
/// image URL/filename a human typed.
pub(super) fn is_adf_media_url(url: &str) -> bool {
    url.starts_with("adf-media://")
}

/// Encode one `media` node's attrs (plus its wrapper, if any) into a
/// complete `![alt](adf-media://...)` Markdown image line.
pub(super) fn encode(media_attrs: &Value, wrapper: Wrapper) -> String {
    let ty = media_attrs
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("file");
    let alt = media_attrs.get("alt").and_then(|a| a.as_str());

    let mut params: Vec<(&str, String)> = Vec::new();
    let base = if ty == "external" {
        if let Some(url) = media_attrs.get("url").and_then(|u| u.as_str()) {
            params.push(("url", url.to_string()));
        }
        SCHEME_EXTERNAL.to_string()
    } else {
        let id = media_attrs.get("id").and_then(|i| i.as_str()).unwrap_or("");
        if let Some(c) = media_attrs.get("collection") {
            params.push(("collection", c.as_str().unwrap_or("").to_string()));
        }
        format!("{SCHEME_FILE}{}", percent_encode(id))
    };

    if let Some(w) = media_attrs.get("width") {
        params.push(("width", number_to_string(w)));
    }
    if let Some(h) = media_attrs.get("height") {
        params.push(("height", number_to_string(h)));
    }
    if let Some(a) = alt {
        params.push(("alt", a.to_string()));
    }
    if let Some(l) = media_attrs.get("localId").and_then(|l| l.as_str()) {
        params.push(("localId", l.to_string()));
    }

    match wrapper {
        Wrapper::None => {}
        Wrapper::Group => params.push(("wrapper", "group".to_string())),
        Wrapper::Single(ms_attrs) => {
            params.push(("wrapper", "single".to_string()));
            if let Some(l) = ms_attrs.get("layout").and_then(|l| l.as_str()) {
                params.push(("layout", l.to_string()));
            }
            if let Some(w) = ms_attrs.get("width") {
                params.push(("msWidth", number_to_string(w)));
            }
            if let Some(t) = ms_attrs.get("widthType").and_then(|t| t.as_str()) {
                params.push(("widthType", t.to_string()));
            }
        }
    }

    let query = params
        .iter()
        .map(|(k, v)| format!("{k}={}", percent_encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    let url = if query.is_empty() {
        base
    } else {
        format!("{base}?{query}")
    };
    // The bracket text is a cosmetic echo only — `decode` always restores
    // `alt` from the query param above, never from here — so a literal `]`
    // (which would otherwise close the Markdown bracket early and make
    // `parse_link`/`whole_line_image` misparse the whole token, losing the
    // media node) is swapped for a look-alike character rather than left
    // to break the round trip.
    let display_alt = alt.unwrap_or("").replace(']', "⟧");
    format!("![{display_alt}]({url})")
}

/// Decode an `adf-media://` URL (the `(url)` half of the image token) back
/// into a `media` node's attrs plus whatever wrapper it was encoded with.
/// Returns `None` if `url` doesn't match a shape this module produced.
pub(super) fn decode(url: &str) -> Option<Decoded> {
    let rest = url.strip_prefix("adf-media://")?;
    let (head, query) = match rest.split_once('?') {
        Some((a, b)) => (a, b),
        None => (rest, ""),
    };
    let params = parse_query(query);
    let mut media_attrs = Map::new();

    if let Some(id) = head.strip_prefix("file/") {
        media_attrs.insert("type".into(), json!("file"));
        media_attrs.insert("id".into(), json!(percent_decode(id)));
        if let Some(c) = params.get("collection") {
            media_attrs.insert("collection".into(), json!(c));
        }
    } else if head == "external" {
        media_attrs.insert("type".into(), json!("external"));
        if let Some(u) = params.get("url") {
            media_attrs.insert("url".into(), json!(u));
        }
    } else {
        return None;
    }

    if let Some(w) = params.get("width") {
        media_attrs.insert("width".into(), number_value(w));
    }
    if let Some(h) = params.get("height") {
        media_attrs.insert("height".into(), number_value(h));
    }
    if let Some(a) = params.get("alt") {
        media_attrs.insert("alt".into(), json!(a));
    }
    if let Some(l) = params.get("localId") {
        media_attrs.insert("localId".into(), json!(l));
    }

    let wrapper = match params.get("wrapper").map(|s| s.as_str()) {
        Some("group") => DecodedWrapper::Group,
        Some("single") => {
            let mut ms = Map::new();
            if let Some(l) = params.get("layout") {
                ms.insert("layout".into(), json!(l));
            }
            if let Some(w) = params.get("msWidth") {
                ms.insert("width".into(), number_value(w));
            }
            if let Some(t) = params.get("widthType") {
                ms.insert("widthType".into(), json!(t));
            }
            DecodedWrapper::Single(Value::Object(ms))
        }
        _ => DecodedWrapper::None,
    };

    Some(Decoded {
        media_attrs: Value::Object(media_attrs),
        wrapper,
    })
}

/// Stringify a JSON number for the query string, preserving whether it was
/// an integer or a float: a whole-number float (e.g. `800.0`) always keeps
/// a decimal point (`f64::to_string` alone would print it as `"800"`,
/// indistinguishable from a real integer) so `number_value` on the other
/// side can tell them apart and rebuild the same `Number` variant —
/// `serde_json`'s int/float `Number` variants aren't `==` to each other
/// even when numerically equal.
fn number_to_string(v: &Value) -> String {
    if let Some(i) = v.as_i64() {
        i.to_string()
    } else if let Some(u) = v.as_u64() {
        u.to_string()
    } else if let Some(f) = v.as_f64() {
        if f.is_finite() && f.fract() == 0.0 {
            format!("{f:.1}")
        } else {
            f.to_string()
        }
    } else {
        v.to_string()
    }
}

/// Parse a query-string number back into a JSON number. A decimal point
/// (or exponent) marks it as originally a float — see `number_to_string` —
/// so only a bare digit string is tried as an integer first.
fn number_value(s: &str) -> Value {
    let looks_like_float = s.contains(['.', 'e', 'E']);
    if !looks_like_float {
        if let Ok(i) = s.parse::<i64>() {
            return json!(i);
        }
    }
    if let Ok(f) = s.parse::<f64>() {
        json!(f)
    } else if let Ok(i) = s.parse::<i64>() {
        json!(i)
    } else {
        json!(s)
    }
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Percent-decode over raw bytes rather than `&str` slicing, so a stray
/// `%` next to a multi-byte UTF-8 sequence can never land a slice off a
/// char boundary and panic.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn parse_query(q: &str) -> std::collections::HashMap<String, String> {
    q.split('&')
        .filter(|s| !s.is_empty())
        .filter_map(|pair| {
            let mut it = pair.splitn(2, '=');
            let k = it.next()?;
            let v = it.next().unwrap_or("");
            Some((k.to_string(), percent_decode(v)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_round_trips_special_characters() {
        let raw = "a & b = c? space, üñî©ødé";
        let encoded = percent_encode(raw);
        assert!(!encoded.contains(' '));
        assert_eq!(percent_decode(&encoded), raw);
    }

    #[test]
    fn decode_rejects_non_adf_media_url() {
        assert!(decode("https://example.com/img.png").is_none());
        assert!(!is_adf_media_url("some-filename.png"));
    }
}
