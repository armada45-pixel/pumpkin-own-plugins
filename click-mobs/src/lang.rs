//! Messages: lang/<language>.json in the data folder (the shipped en_US/de_DE files are written there on first use),
//! fallback en_US, `{param}` placeholders, `&` colour codes, same message types as the original MessageType.

use serde_json::Value;
use std::collections::HashMap;

const EN_US: &str = include_str!("../lang/en_US.json");
const DE_DE: &str = include_str!("../lang/de_DE.json");

pub struct Lang {
    language: String,
    map: HashMap<String, String>,
    fallback: HashMap<String, String>,
}

fn flatten(text: &str) -> HashMap<String, String> {
    let Ok(Value::Object(obj)) = serde_json::from_str::<Value>(text) else {
        return HashMap::new();
    };
    obj.into_iter()
        .filter_map(|(k, v)| match v {
            Value::String(s) => Some((k, s)),
            // A list is one text with a line per entry (mob.lore).
            Value::Array(a) => Some((
                k,
                a.iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join("\n"),
            )),
            _ => None,
        })
        .collect()
}

impl Lang {
    pub fn load(data_dir: &str, language: &str) -> Self {
        let shipped = match language {
            "en_US" => Some(EN_US),
            "de_DE" => Some(DE_DE),
            _ => None,
        };
        let path = format!("{data_dir}/lang/{language}.json");
        let text = std::fs::read_to_string(&path).ok().or_else(|| {
            let text = shipped?;
            let _ = std::fs::create_dir_all(format!("{data_dir}/lang"));
            let _ = std::fs::write(&path, text);
            Some(text.to_string())
        });
        Self {
            language: language.into(),
            map: text.as_deref().map(flatten).unwrap_or_default(),
            fallback: flatten(EN_US),
        }
    }

    pub fn language(&self) -> &str {
        &self.language
    }

    /// Localized text with `{name}` replaced. A key missing everywhere gives the key, like the original.
    pub fn get(&self, key: &str, params: &[(&str, &str)]) -> String {
        let mut text = self
            .map
            .get(key)
            .or_else(|| self.fallback.get(key))
            .cloned()
            .unwrap_or_else(|| key.to_string());
        for (name, value) in params {
            text = text.replace(&format!("{{{name}}}"), value);
        }
        text
    }
}

/// `ChatColor.translateAlternateColorCodes('&', text)`.
pub fn colorize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let code = chars
            .peek()
            .is_some_and(|n| n.is_ascii_hexdigit() || "klmnorKLMNOR".contains(*n));
        out.push(if c == '&' && code { '§' } else { c });
    }
    out
}

/// A message kind: chat prefix, action-bar format and whether it plays a sound (original MessageType).
#[derive(Clone, Copy)]
pub enum Kind {
    Fail,
    PickUp,
    Config,
}

impl Kind {
    pub fn prefix(self) -> &'static str {
        match self {
            Kind::Fail => "&c[❌] ",
            Kind::PickUp => "&2[↑] &a",
            Kind::Config => "&6[⚒] &a",
        }
    }

    pub fn title_format(self) -> &'static str {
        match self {
            Kind::Fail => "&8< &c%s &8>",
            Kind::PickUp => "&8< &2↑ &a%s &8>",
            Kind::Config => "&8< &6%s &8>",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colorize_translates_codes_only() {
        assert_eq!(colorize("&aHi &lthere & x &"), "§aHi §lthere & x &");
    }

    #[test]
    fn lore_is_one_text_with_lines() {
        let m = flatten(DE_DE);
        assert_eq!(
            m["mob.lore"],
            "§8Rechtsklick, um den {mob}\n§8zurück zu platzieren."
        );
    }
}
