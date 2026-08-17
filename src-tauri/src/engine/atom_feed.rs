use serde::{Deserialize, Serialize};
use quick_xml::events::Event;
use quick_xml::reader::Reader;

pub const ATOM_FEED_URL: &str = "https://github.com/TitoTFP/WuwaID/releases.atom";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseNoteEntry {
    pub tag: String,
    pub date: String,
    pub title: String,
    pub body: String,
    pub author: String,
}

fn unescape_text(bytes: &quick_xml::events::BytesText) -> Result<String, String> {
    let s = String::from_utf8_lossy(bytes);
    quick_xml::escape::unescape(&s)
        .map(|c| c.to_string())
        .map_err(|e| format!("Unescape error: {}", e))
}

/// Parses an RFC 4287 Atom XML feed and extracts the latest entry using quick-xml.
pub fn parse_atom_feed(xml: &str) -> Result<ReleaseNoteEntry, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut in_entry = false;

    let mut tag = String::new();
    let mut date = String::new();
    let mut title = String::new();
    let mut body = String::new();
    let mut author = String::new();

    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                if name == "entry" {
                    in_entry = true;
                } else if in_entry {
                    match name.as_str() {
                        "id" => {
                            let text_bytes = reader.read_text(e.to_end().name()).map_err(|e| format!("{}", e))?;
                            let text = unescape_text(&text_bytes)?;
                            if let Some(pos) = text.rfind('/') {
                                tag = text[pos + 1..].to_string();
                            } else {
                                tag = text;
                            }
                        }
                        "title" => {
                            let text_bytes = reader.read_text(e.to_end().name()).map_err(|e| format!("{}", e))?;
                            title = unescape_text(&text_bytes)?;
                        }
                        "updated" => {
                            let text_bytes = reader.read_text(e.to_end().name()).map_err(|e| format!("{}", e))?;
                            date = unescape_text(&text_bytes)?;
                        }
                        "name" => {
                            let text_bytes = reader.read_text(e.to_end().name()).map_err(|e| format!("{}", e))?;
                            author = unescape_text(&text_bytes)?;
                        }
                        "content" => {
                            let text_bytes = reader.read_text(e.to_end().name()).map_err(|e| format!("{}", e))?;
                            body = unescape_text(&text_bytes)?;
                        }
                        _ => {}
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                if name == "entry" {
                    break;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("XML parser error: {}", e)),
            _ => {}
        }
        buf.clear();
    }

    if tag.is_empty() && title.is_empty() {
        return Err("No valid <entry> found in Atom feed".to_string());
    }

    Ok(ReleaseNoteEntry {
        tag,
        date,
        title,
        body,
        author,
    })
}

pub async fn fetch_latest_release_notes(client: &reqwest::Client, url: &str) -> Result<ReleaseNoteEntry, String> {
    let resp = client
        .get(url)
        .header("User-Agent", "WuwaIDLauncher-Tauri")
        .send()
        .await
        .map_err(|e| format!("Gagal mengambil Atom feed release notes: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Server response error: {}", resp.status()));
    }

    let text = resp
        .text()
        .await
        .map_err(|e| format!("Gagal membaca response Atom feed: {}", e))?;

    parse_atom_feed(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_atom_feed_basic() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <feed xmlns="http://www.w3.org/2005/Atom">
          <title>Release notes</title>
          <entry>
            <id>tag:github.com,2008:Repository/1228981298/v3.5.1-id.3</id>
            <updated>2026-07-18T12:54:15Z</updated>
            <title>Wuthering Waves Lokalisasi Bahasa Indonesia v.3.5.1-id.3</title>
            <content type="html">&lt;h1&gt;Judul&lt;/h1&gt;&lt;p&gt;Deskripsi patch&lt;/p&gt;</content>
            <author><name>TitoTFP</name></author>
          </entry>
        </feed>"#;

        let entry = parse_atom_feed(xml).unwrap();
        assert_eq!(entry.tag, "v3.5.1-id.3");
        assert_eq!(entry.title, "Wuthering Waves Lokalisasi Bahasa Indonesia v.3.5.1-id.3");
        assert_eq!(entry.author, "TitoTFP");
        assert!(entry.body.contains("<h1>Judul</h1>"));
    }
}
