use serde::{de, Deserializer, Serializer};
use std::fmt;

const STANDARD_CHARS: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn encode(data: &[u8]) -> String {
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    let mut i = 0;
    while i < data.len() {
        let b0 = data[i];
        let b1 = if i + 1 < data.len() { data[i + 1] } else { 0 };
        let b2 = if i + 2 < data.len() { data[i + 2] } else { 0 };

        let triple = ((b0 as u32) << 16) | ((b1 as u32) << 8) | (b2 as u32);

        out.push(STANDARD_CHARS[((triple >> 18) & 0x3F) as usize] as char);
        out.push(STANDARD_CHARS[((triple >> 12) & 0x3F) as usize] as char);

        if i + 1 < data.len() {
            out.push(STANDARD_CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }

        if i + 2 < data.len() {
            out.push(STANDARD_CHARS[(triple & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }

        i += 3;
    }
    out
}

fn decode_char(c: u8) -> Result<u8, &'static str> {
    match c {
        b'A'..=b'Z' => Ok(c - b'A'),
        b'a'..=b'z' => Ok(c - b'a' + 26),
        b'0'..=b'9' => Ok(c - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        _ => Err("invalid base64 char"),
    }
}

pub fn decode(input: &str) -> Result<Vec<u8>, &'static str> {
    let raw_bytes = input.as_bytes();
    if raw_bytes.is_empty() {
        return Ok(Vec::new());
    }

    // Go base64.StdEncoding ignores \r and \n, but rejects other whitespace like spaces and tabs.
    let mut filtered = Vec::with_capacity(raw_bytes.len());
    for &b in raw_bytes {
        if b == b'\r' || b == b'\n' {
            continue;
        }
        filtered.push(b);
    }

    if filtered.is_empty() {
        return Ok(Vec::new());
    }
    if filtered.len() % 4 != 0 {
        return Err("invalid base64 length");
    }

    let bytes = &filtered;
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    let mut i = 0;
    while i < bytes.len() {
        let c0 = decode_char(bytes[i])?;
        let c1 = decode_char(bytes[i + 1])?;

        let has_pad2 = bytes[i + 2] == b'=';
        let has_pad3 = bytes[i + 3] == b'=';

        if has_pad2 && !has_pad3 {
            return Err("invalid base64 padding");
        }

        let c2 = if has_pad2 {
            0
        } else {
            decode_char(bytes[i + 2])?
        };
        let c3 = if has_pad3 {
            0
        } else {
            decode_char(bytes[i + 3])?
        };

        let triple = ((c0 as u32) << 18) | ((c1 as u32) << 12) | ((c2 as u32) << 6) | (c3 as u32);

        out.push(((triple >> 16) & 0xFF) as u8);
        if !has_pad2 {
            out.push(((triple >> 8) & 0xFF) as u8);
        }
        if !has_pad3 {
            out.push((triple & 0xFF) as u8);
        }

        i += 4;
    }
    Ok(out)
}

pub fn serialize<S>(opt: &Option<Vec<u8>>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match opt {
        Some(bytes) => {
            let encoded = encode(bytes);
            serializer.serialize_str(&encoded)
        }
        None => serializer.serialize_none(),
    }
}

pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Vec<u8>>, D::Error>
where
    D: Deserializer<'de>,
{
    struct OptBase64Visitor;

    impl<'de> de::Visitor<'de> for OptBase64Visitor {
        type Value = Option<Vec<u8>>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a base64-encoded string or null")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(None)
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_string(self)
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(None)
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            if v.is_empty() {
                return Ok(Some(Vec::new()));
            }
            decode(v).map(Some).map_err(de::Error::custom)
        }

        fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.visit_str(&v)
        }
    }

    deserializer.deserialize_option(OptBase64Visitor)
}
