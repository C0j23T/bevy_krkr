//! The XML files the engine's config managers read and write.

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Element {
    tag: String,
    /// Name and unescaped value, in document order.
    attributes: Vec<(String, String)>,
}

impl Element {
    fn from_start(start: &BytesStart<'_>) -> Option<Self> {
        let tag = std::str::from_utf8(start.name().as_ref()).ok()?.to_owned();
        let mut attributes = Vec::new();
        for attribute in start.attributes() {
            let attribute = attribute.ok()?;
            let name = std::str::from_utf8(attribute.key.as_ref()).ok()?.to_owned();
            // These files are XML 1.0 from first line to last, and none of them
            // has a DTD or an entity outside the predefined set.
            let value = attribute.normalized_value(XmlVersion::Implicit1_0).ok()?.into_owned();
            attributes.push((name, value));
        }
        Some(Element { tag, attributes })
    }

    fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes.iter().find(|(attribute, _)| attribute == name).map(|(_, value)| value.as_str())
    }
}

/// The root element's children; anything nested below them belongs to
/// whatever it is inside and is not a record.
#[derive(Debug, Default, Clone)]
struct Document {
    children: Vec<Element>,
}

impl Document {
    /// `None` means the document is not well-formed.
    fn parse(xml: &[u8]) -> Option<Self> {
        let xml = xml.strip_prefix(b"\xEF\xBB\xBF".as_slice()).unwrap_or(xml);
        let mut reader = Reader::from_reader(xml);
        let mut children = Vec::new();
        let mut current = None;
        // Elements open at this point of the stream; 1 is the root itself.
        let mut depth = 0usize;
        let mut rooted = false;
        loop {
            match reader.read_event().ok()? {
                Event::Start(start) => {
                    if depth == 0 {
                        rooted = true;
                    }
                    if depth == 1 {
                        current = Some(Element::from_start(&start)?);
                    }
                    depth += 1;
                }
                Event::Empty(start) => {
                    if depth == 1 {
                        children.push(Element::from_start(&start)?);
                    }
                }
                Event::End(_) => {
                    depth = depth.checked_sub(1)?;
                    if depth == 1 {
                        children.push(current.take()?);
                    }
                }
                Event::Eof => break,
                _ => {}
            }
        }
        // quick-xml accepts an empty stream and an unclosed root element; both
        // are "not XML".
        if !rooted || depth != 0 {
            return None;
        }
        Some(Document { children })
    }

    fn pairs<'a>(&'a self, tag: &str) -> impl Iterator<Item = (&'a str, &'a str)> + 'a {
        // Owned so that the iterator borrows only `self`.
        let tag = tag.to_owned();
        self.children
            .iter()
            .filter(move |element| element.tag == tag)
            .filter_map(|element| Some((element.attribute("key")?, element.attribute("value")?)))
    }

    fn texts(&self) -> impl Iterator<Item = (&str, &str)> + '_ {
        self.children
            .iter()
            .filter_map(|element| Some((element.attribute("id")?, element.attribute("text")?)))
    }
}

struct Writer {
    root: String,
    records: Vec<(String, Vec<(String, String)>)>,
}

impl Writer {
    fn new(root: &str) -> Self {
        Writer { root: root.to_owned(), records: Vec::new() }
    }

    fn record(&mut self, tag: &str, attributes: Vec<(String, String)>) {
        self.records.push((tag.to_owned(), attributes));
    }

    fn finish(&self) -> String {
        let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        out.push('<');
        out.push_str(&self.root);
        out.push_str(">\n");
        for (tag, attributes) in &self.records {
            out.push_str("  <");
            out.push_str(tag);
            for (name, value) in attributes {
                out.push(' ');
                out.push_str(name);
                out.push_str("=\"");
                escape_into(&mut out, value);
                out.push('"');
            }
            out.push_str("/>\n");
        }
        out.push_str("</");
        out.push_str(&self.root);
        out.push_str(">\n");
        out
    }
}

fn escape_into(out: &mut String, text: &str) {
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
}

/// `atoi` semantics: the leading number, 0 when there is none. A key map that
/// was hand-edited is not guaranteed to be spelled cleanly.
fn leading_int(text: &str) -> i32 {
    let text = text.trim_start();
    let end = text
        .char_indices()
        .find(|&(index, ch)| !(ch.is_ascii_digit() || (index == 0 && (ch == '+' || ch == '-'))))
        .map_or(text.len(), |(index, _)| index);
    text[..end].parse().unwrap_or(0)
}

/// A setting, or an extra command-line argument.
pub type Entry = (String, String);

#[derive(Debug, Default, Clone)]
pub struct Preference {
    pub items: Vec<Entry>,
    pub custom_arguments: Vec<Entry>,
    /// Key code before and after the remap. A pair with a zero is dropped: the
    /// engine's key map uses zero for "unmapped".
    pub keymap: Vec<(i32, i32)>,
}

impl Preference {
    /// `None` when the contents are not a well-formed XML document.
    pub fn parse(xml: &[u8]) -> Option<Self> {
        let document = Document::parse(xml)?;
        let pairs = |tag: &str| -> Vec<Entry> {
            document.pairs(tag).map(|(key, value)| (key.to_owned(), value.to_owned())).collect()
        };
        Some(Preference {
            items: pairs("Item"),
            custom_arguments: pairs("Custom"),
            keymap: document
                .pairs("KeyMap")
                .map(|(key, value)| (leading_int(key), leading_int(value)))
                .filter(|&(key, value)| key != 0 && value != 0)
                .collect(),
        })
    }

    /// Records keep the order they were read or pushed in, which for a key map
    /// is the engine's `std::map` order.
    pub fn to_xml(&self) -> String {
        let mut writer = Writer::new("GlobalPreference");
        let mut record = |tag: &str, key: &str, value: &str| {
            writer.record(
                tag,
                vec![
                    (String::from("key"), key.to_owned()),
                    (String::from("value"), value.to_owned()),
                ],
            );
        };
        for (key, value) in &self.items {
            record("Item", key, value);
        }
        for (key, value) in &self.custom_arguments {
            record("Custom", key, value);
        }
        for (key, value) in &self.keymap {
            record("KeyMap", &key.to_string(), &value.to_string());
        }
        writer.finish()
    }
}

#[derive(Debug, Default, Clone)]
pub struct Locale {
    /// `(id, text)`, in file order.
    pub texts: Vec<Entry>,
}

impl Locale {
    /// `None` when the contents are not a well-formed XML document.
    pub fn parse(xml: &[u8]) -> Option<Self> {
        let document = Document::parse(xml)?;
        Some(Locale {
            texts: document.texts().map(|(id, text)| (id.to_owned(), text.to_owned())).collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One line and one root, the way the engine writes it.
    const SETTINGS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\" ?>\n\
        <GlobalPreference>\n\
        \x20   <Item key=\"user_language\" value=\"jp\"/>\n\
        \x20   <Item key=\"display_mode\" value=\"fullscreen\"/>\n\
        \x20   <Custom key=\"window\" value=\"normal\"/>\n\
        \x20   <KeyMap key=\"13\" value=\"27\"/>\n\
        </GlobalPreference>\n";

    fn parse(settings: &str) -> Preference {
        Preference::parse(settings.as_bytes()).expect("parses")
    }

    #[test]
    fn reads_the_three_lists() {
        let preference = parse(SETTINGS);
        assert_eq!(
            preference.items,
            vec![
                (String::from("user_language"), String::from("jp")),
                (String::from("display_mode"), String::from("fullscreen")),
            ]
        );
        assert_eq!(
            preference.custom_arguments,
            vec![(String::from("window"), String::from("normal"))]
        );
        assert_eq!(preference.keymap, vec![(13, 27)]);
    }

    /// A key map uses zero for "unmapped", so a pair with a zero is not an
    /// entry.
    #[test]
    fn key_map_entries_need_two_numbers() {
        let xml = "<Preference>\
            <KeyMap key=\"13\" value=\"27\"/>\
            <KeyMap key=\" 14 \" value=\"28junk\"/>\
            <KeyMap key=\"0\" value=\"29\"/>\
            <KeyMap key=\"15\" value=\"0\"/>\
            <KeyMap key=\"abc\" value=\"30\"/>\
            <KeyMap key=\"\" value=\"\"/>\
            </Preference>";
        assert_eq!(parse(xml).keymap, vec![(13, 27), (14, 28)]);
    }

    #[test]
    fn ignores_nested_elements() {
        let xml = "<Root><Item key=\"a\" value=\"1\"><Item key=\"b\" value=\"2\"/></Item></Root>";
        assert_eq!(parse(xml).items, vec![(String::from("a"), String::from("1"))]);
    }

    #[test]
    fn skips_the_bom() {
        let mut xml = b"\xEF\xBB\xBF".to_vec();
        xml.extend_from_slice(SETTINGS.as_bytes());
        assert_eq!(Preference::parse(&xml).expect("parses").items.len(), 2);
    }

    /// An empty file is what a failed save leaves behind.
    #[test]
    fn rejects_malformed_documents() {
        assert!(Preference::parse(b"<Root><Item></Root>").is_none());
        assert!(Preference::parse(b"").is_none());
        assert!(Locale::parse(b"<Root><Name>").is_none());
    }

    /// The next run reads this file, so the two halves have to agree, entities
    /// included.
    #[test]
    fn writer_roundtrips_through_the_parser() {
        let preference = Preference {
            items: vec![
                (String::from("user_language"), String::from("jp")),
                (String::from("path"), String::from(r#"C:\a&b"<c>"#)),
            ],
            custom_arguments: vec![(String::from("window"), String::from("normal"))],
            keymap: vec![(13, 27)],
        };
        let reread = Preference::parse(preference.to_xml().as_bytes()).expect("parses");
        assert_eq!(reread.items, preference.items);
        assert_eq!(reread.custom_arguments, preference.custom_arguments);
        assert_eq!(reread.keymap, preference.keymap);
    }

    #[test]
    fn locale_reads_every_element_with_an_id_and_a_text() {
        let xml = "<Locale>\
            <Text id=\"ok\" text=\"OK\"/>\
            <Text id=\"cancel\" text=\"Cancel &amp; back\"/>\
            <Text id=\"broken\"/>\
            <Text text=\"no id\"/>\
            <Group id=\"group\" text=\"ignored\"><Text id=\"nested\" text=\"nested\"/></Group>\
            </Locale>";
        assert_eq!(
            Locale::parse(xml.as_bytes()).expect("parses").texts,
            vec![
                (String::from("ok"), String::from("OK")),
                (String::from("cancel"), String::from("Cancel & back")),
                (String::from("group"), String::from("ignored")),
            ]
        );
    }
}
