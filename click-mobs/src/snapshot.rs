//! What is stored on the mob item and the mob head texture.
//!
//! The original stores a full entity snapshot (`EntitySnapshot#getAsString`, SNBT) in the item's persistent data. The
//! host has no entity NBT read/write yet (HOST-API.md item 1), so this port stores the same three keys with a reduced
//! SNBT (`id`, `CustomName`, `Age`) and restores those fields only.

use crate::textures::{DEFAULT_TEXTURE, TEXTURES};

/// Namespace of the keys (`clickmobs:entity`, `clickmobs:type`, `clickmobs:nbt` in the original).
pub const NS: &str = "clickmobs";

pub fn write_nbt(key: &str, custom_name: Option<&str>, age: i32) -> String {
    let mut s = format!("{{id:\"minecraft:{key}\"");
    if let Some(name) = custom_name {
        let json = serde_json::json!({ "text": name }).to_string();
        s += &format!(
            ",CustomName:'{}'",
            json.replace('\\', "\\\\").replace('\'', "\\'")
        );
    }
    if age != 0 {
        s += &format!(",Age:{age}");
    }
    s + "}"
}

/// Reads `CustomName` and `Age` back from an SNBT string (ours or the original's).
pub fn read_nbt(nbt: &str) -> (Option<String>, i32) {
    let age = nbt
        .find("Age:")
        .map(|i| &nbt[i + 4..])
        .and_then(|t| {
            let end = t
                .find(|c: char| !(c.is_ascii_digit() || c == '-'))
                .unwrap_or(t.len());
            t[..end].parse().ok()
        })
        .unwrap_or(0);
    (custom_name(nbt), age)
}

fn custom_name(nbt: &str) -> Option<String> {
    let t = &nbt[nbt.find("CustomName:")? + "CustomName:".len()..];
    let quote = t.chars().next().filter(|c| matches!(c, '\'' | '"'))?;
    let mut raw = String::new();
    let mut chars = t[1..].chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => raw.push(chars.next()?),
            c if c == quote => break,
            c => raw.push(c),
        }
    }
    // A JSON text component or a plain string.
    match serde_json::from_str::<serde_json::Value>(&raw) {
        Ok(serde_json::Value::Object(o)) => o.get("text")?.as_str().map(String::from),
        Ok(serde_json::Value::String(s)) => Some(s),
        _ => Some(raw),
    }
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn write_string(out: &mut Vec<u8>, s: &str) {
    write_var_int(out, s.len() as u32);
    out.extend_from_slice(s.as_bytes());
}

fn write_var_int(out: &mut Vec<u8>, mut v: u32) {
    loop {
        let b = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}

/// Network encoding of the `profile` component: a profile with one `textures` property (random id, like
/// `Bukkit.createPlayerProfile(UUID.randomUUID())` + `setSkin(url)`).
pub fn profile_component(entity_key: &str, uuid_seed: u128) -> Vec<u8> {
    let hash = TEXTURES
        .iter()
        .find(|(k, _)| *k == entity_key)
        .map_or(DEFAULT_TEXTURE, |(_, h)| h);
    let json = format!(
        r#"{{"textures":{{"SKIN":{{"url":"http://textures.minecraft.net/texture/{hash}"}}}}}}"#
    );
    let mut out = vec![0, 0, 1];
    out.extend_from_slice(&uuid_seed.to_be_bytes());
    write_var_int(&mut out, 1);
    write_string(&mut out, "textures");
    write_string(&mut out, &base64(json.as_bytes()));
    out.extend_from_slice(&[0, 0, 0, 0, 0]); // no signature, texture, cape, elytra, model
    out
}

/// `custom_model_data` component with one float.
pub fn model_data_component(value: i32) -> Vec<u8> {
    let mut out = vec![1];
    out.extend_from_slice(&(value as f32).to_be_bytes());
    out.extend_from_slice(&[0, 0, 0]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nbt_round_trip() {
        let s = write_nbt("cow", Some("Bob's \"cow\""), -24000);
        assert_eq!(read_nbt(&s), (Some("Bob's \"cow\"".into()), -24000));
        assert_eq!(read_nbt(&write_nbt("pig", None, 0)), (None, 0));
        assert_eq!(
            read_nbt("{CustomName:\"Plain\",Age:5}"),
            (Some("Plain".into()), 5)
        );
    }

    #[test]
    fn base64_matches_reference() {
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
    }
}
