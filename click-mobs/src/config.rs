//! config.yml of ClickMobs: same keys, defaults and comments as the original (Configured library).

use serde_yaml_ng::Value;
use std::collections::BTreeMap;

const TEMPLATE: &str = include_str!("config-template.yml");

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub language: String,
    pub check_update: bool,
    pub per_mob_permissions: bool,
    pub enable_dispensers: bool,
    pub whitelisted_mobs: Vec<String>,
    pub blacklisted_mobs: Vec<String>,
    pub blacklisted_items_in_hand: Vec<String>,
    pub custom_model_data: BTreeMap<String, i32>,
}

impl Default for Config {
    fn default() -> Self {
        let list = |v: &[&str]| v.iter().map(ToString::to_string).collect();
        Self {
            language: "en_US".into(),
            check_update: true,
            per_mob_permissions: false,
            enable_dispensers: true,
            whitelisted_mobs: list(&["cow", "pig", "sheep"]),
            blacklisted_mobs: list(&["?hostile", "wither", "ender_dragon"]),
            blacklisted_items_in_hand: list(&["lead", "saddle"]),
            custom_model_data: BTreeMap::from([("creeper".into(), 0), ("skeleton".into(), 0)]),
        }
    }
}

/// Options with their `/clickmobs get` key, in file order. `_version` is a registered option too.
pub const OPTIONS: [&str; 9] = [
    "_version",
    "language",
    "check_update",
    "per_mob_permissions",
    "enable_dispensers",
    "whitelisted_mobs",
    "blacklisted_mobs",
    "blacklisted_items_in_hand",
    "custom_model_data",
];
/// Options `/clickmobs set` offers (boolean, number or string defaults).
pub const SETTABLE: [&str; 4] = [
    "language",
    "check_update",
    "per_mob_permissions",
    "enable_dispensers",
];

fn strings(v: Option<&Value>) -> Option<Vec<String>> {
    v?.as_sequence()?
        .iter()
        .map(|x| match x {
            Value::String(s) => Some(s.clone()),
            other => serde_yaml_ng::to_string(other)
                .ok()
                .map(|s| s.trim().to_string()),
        })
        .collect()
}

impl Config {
    /// Parses a config file. Returns the config and whether a key was missing (the original appends defaults).
    pub fn parse(text: &str) -> (Self, bool) {
        let d = Self::default();
        let root: Value = serde_yaml_ng::from_str(text).unwrap_or(Value::Null);
        let mut missing = false;
        let mut get = |key: &str| {
            let v = root.get(key);
            missing |= v.is_none();
            v
        };
        let cfg = Self {
            language: get("language")
                .and_then(Value::as_str)
                .map(String::from)
                .unwrap_or(d.language),
            check_update: get("check_update")
                .and_then(Value::as_bool)
                .unwrap_or(d.check_update),
            per_mob_permissions: get("per_mob_permissions")
                .and_then(Value::as_bool)
                .unwrap_or(d.per_mob_permissions),
            enable_dispensers: get("enable_dispensers")
                .and_then(Value::as_bool)
                .unwrap_or(d.enable_dispensers),
            whitelisted_mobs: strings(get("whitelisted_mobs")).unwrap_or(d.whitelisted_mobs),
            blacklisted_mobs: strings(get("blacklisted_mobs")).unwrap_or(d.blacklisted_mobs),
            blacklisted_items_in_hand: strings(get("blacklisted_items_in_hand"))
                .unwrap_or(d.blacklisted_items_in_hand),
            custom_model_data: get("custom_model_data")
                .and_then(Value::as_mapping)
                .map(|m| {
                    m.iter()
                        .filter_map(|(k, v)| {
                            Some((k.as_str()?.to_string(), i32::try_from(v.as_i64()?).ok()?))
                        })
                        .collect()
                })
                .unwrap_or(d.custom_model_data),
        };
        missing |= root.get("_version").is_none();
        (cfg, missing)
    }

    /// Renders the full file with the original comments.
    pub fn render(&self) -> String {
        let list = |v: &[String]| v.iter().map(|s| format!("\n- {s}")).collect::<String>();
        let map = self
            .custom_model_data
            .iter()
            .map(|(k, v)| format!("\n  {k}: {v}"))
            .collect::<String>();
        TEMPLATE
            .replace("{{language}}", &self.language)
            .replace("{{check_update}}", &self.check_update.to_string())
            .replace(
                "{{per_mob_permissions}}",
                &self.per_mob_permissions.to_string(),
            )
            .replace("{{enable_dispensers}}", &self.enable_dispensers.to_string())
            .replace("{{whitelisted_mobs}}", &list(&self.whitelisted_mobs))
            .replace("{{blacklisted_mobs}}", &list(&self.blacklisted_mobs))
            .replace(
                "{{blacklisted_items_in_hand}}",
                &list(&self.blacklisted_items_in_hand),
            )
            .replace("{{custom_model_data}}", &map)
    }

    /// Value as Java's `toString` prints it (what `/clickmobs get` shows).
    pub fn display(&self, key: &str) -> Option<String> {
        let list = |v: &[String]| format!("[{}]", v.join(", "));
        Some(match key {
            "_version" => "8".into(),
            "language" => self.language.clone(),
            "check_update" => self.check_update.to_string(),
            "per_mob_permissions" => self.per_mob_permissions.to_string(),
            "enable_dispensers" => self.enable_dispensers.to_string(),
            "whitelisted_mobs" => list(&self.whitelisted_mobs),
            "blacklisted_mobs" => list(&self.blacklisted_mobs),
            "blacklisted_items_in_hand" => list(&self.blacklisted_items_in_hand),
            "custom_model_data" => {
                format!(
                    "{{{}}}",
                    self.custom_model_data
                        .iter()
                        .map(|(k, v)| format!("{k}={v}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_render_round_trips_and_matches_shipped_file() {
        let text = Config::default().render();
        let (cfg, missing) = Config::parse(&text);
        assert_eq!(cfg, Config::default());
        assert!(!missing);
        assert!(text.starts_with("# ----"));
        assert!(text.contains("blacklisted_mobs:\n- ?hostile\n- wither\n- ender_dragon\n"));
        assert!(text.ends_with("custom_model_data:\n  creeper: 0\n  skeleton: 0\n"));
    }
}
