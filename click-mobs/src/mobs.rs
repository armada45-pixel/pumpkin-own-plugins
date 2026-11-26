//! Mob lists (whitelist/blacklist) and the `?tag` predicates, parsed like the original MobListParser.

use crate::entity_types::ALL;
use pumpkin_plugin_api::{Entity, common::NbtTag, mobs::MobData};
use std::collections::HashSet;

/// What a predicate may ask about a mob.
pub struct Subject {
    pub key: String,
    pub custom_name: Option<String>,
    pub baby: bool,
    pub tamed: bool,
    pub silent: bool,
    pub entity: Entity,
}

impl Subject {
    pub fn new(entity: Entity) -> Self {
        let key = key_of(&entity);
        let mob_data = entity.as_mob().map(|m| m.get_mob_data());
        let (baby, tamed) = match mob_data {
            Some(MobData::Ageable(d)) => (d.is_baby, false),
            Some(MobData::Zombie(d)) => (d.is_baby, false),
            Some(MobData::Wolf(d)) => (false, d.is_tamed),
            Some(MobData::Cat(d)) => (false, d.is_tamed),
            _ => (false, false),
        };
        Self {
            custom_name: entity.get_custom_name().map(|n| n.get_text()),
            baby,
            tamed,
            silent: entity.is_silent(),
            key,
            entity,
        }
    }

    // Bukkit's `Monster` interface.
    fn hostile(&self) -> bool {
        const MONSTERS: &[&str] = &[
            "blaze",
            "bogged",
            "breeze",
            "cave_spider",
            "creaking",
            "creeper",
            "drowned",
            "elder_guardian",
            "endermite",
            "enderman",
            "evoker",
            "giant",
            "guardian",
            "husk",
            "illusioner",
            "parched",
            "piglin",
            "piglin_brute",
            "pillager",
            "ravager",
            "silverfish",
            "skeleton",
            "spider",
            "stray",
            "vex",
            "vindicator",
            "warden",
            "witch",
            "wither",
            "wither_skeleton",
            "zoglin",
            "zombie",
            "zombie_villager",
            "zombified_piglin",
        ];
        MONSTERS.contains(&self.key.as_str())
    }
}

/// Registry key of an entity without namespace, e.g. `cave_spider`.
pub fn key_of(entity: &Entity) -> String {
    let t = entity.get_type();
    ALL.iter()
        .find(|(_, e)| *e == t)
        .map_or_else(|| "unknown".into(), |(k, _)| (*k).into())
}

enum Kind {
    All,
    Hostile,
    Baby,
    Tamed,
    Nametagged,
    Silent,
    Mob,
    Leashed,
    CustomData,
}

enum Predicate {
    Single(Kind, Vec<String>),
    Not(Box<Predicate>),
    All(Vec<Predicate>),
}

impl Predicate {
    fn test(&self, s: &Subject) -> bool {
        match self {
            Predicate::Not(p) => !p.test(s),
            Predicate::All(ps) => ps.iter().all(|p| p.test(s)),
            Predicate::Single(kind, args) => match kind {
                Kind::All => true,
                Kind::Hostile => s.hostile(),
                Kind::Baby => s.baby,
                Kind::Tamed => s.tamed,
                Kind::Nametagged => match (&s.custom_name, args.first()) {
                    (Some(_), None) => true,
                    (Some(n), Some(want)) => n == want,
                    (None, _) => false,
                },
                Kind::Silent => s.silent,
                Kind::Mob => args.iter().any(|a| a.eq_ignore_ascii_case(&s.key)),
                // shortcut: the host has no leash API, so nothing counts as leashed (HOST-API.md).
                Kind::Leashed => false,
                Kind::CustomData => args.iter().any(|a| custom_data_matches(s, a)),
            },
        }
    }
}

/// `?customdata(namespace:key)` or `?customdata(namespace:key=type[value])`, against the entity's custom data.
fn custom_data_matches(s: &Subject, arg: &str) -> bool {
    let (key, rest) = arg
        .split_once('=')
        .map_or((arg, None), |(k, r)| (k, Some(r)));
    let (ns, name) = key.split_once(':').unwrap_or(("minecraft", key));
    let Some(tree) = s.entity.get_custom_data(ns, name) else {
        return false;
    };
    let Some(rest) = rest else { return true };
    let Some((ty, value)) = rest
        .split_once('[')
        .and_then(|(t, v)| Some((t, v.strip_suffix(']')?)))
    else {
        return false;
    };
    let Some(tag) = tree.tags.get(tree.root as usize) else {
        return false;
    };
    match (ty.to_lowercase().as_str(), tag) {
        ("string", NbtTag::StringTag(v)) => v == value,
        ("integer" | "int", NbtTag::Int(v)) => value.parse() == Ok(*v),
        ("long", NbtTag::Long(v)) => value.parse() == Ok(*v),
        ("double", NbtTag::Double(v)) => value.parse() == Ok(*v),
        ("float", NbtTag::Float(v)) => value.parse() == Ok(*v),
        ("short", NbtTag::Short(v)) => value.parse() == Ok(*v),
        ("byte", NbtTag::Byte(v)) => value.parse() == Ok(*v),
        ("boolean" | "bool", NbtTag::Byte(v)) => (*v != 0) == (value == "true"),
        _ => false,
    }
}

#[derive(Default)]
pub struct MobList {
    mobs: HashSet<String>,
    predicates: Vec<Predicate>,
}

impl MobList {
    pub fn parse(lines: &[String]) -> Self {
        let mut list = Self::default();
        for line in lines {
            if line.contains('?') {
                match parse_predicate(line) {
                    Ok(p) => list.predicates.push(p),
                    Err(e) => {
                        tracing::error!("Error parsing mob predicate/line: {line}. Error: {e}")
                    }
                }
            } else {
                list.mobs.insert(line.clone());
            }
        }
        list
    }

    pub fn contains(&self, s: &Subject) -> bool {
        self.mobs.contains(&s.key) || self.predicates.iter().any(|p| p.test(s))
    }
}

/// `(not )?\?name(args)?` repeated; same grammar as the original regex.
fn parse_predicate(line: &str) -> Result<Predicate, String> {
    let mut parts = Vec::new();
    let mut rest = line;
    while let Some(q) = rest.find('?') {
        let negated =
            rest[..q].trim_end().ends_with("not") && rest[..q].ends_with(char::is_whitespace);
        let after = &rest[q + 1..];
        let name_len = after
            .find(|c: char| !(c.is_ascii_lowercase() || c == '_'))
            .unwrap_or(after.len());
        let name = &after[..name_len];
        let mut tail = &after[name_len..];
        let mut args = Vec::new();
        // `(?<args>.+?)` up to the first `)`.
        if let Some(inner) = tail.strip_prefix('(')
            && let Some(end) = inner.find(')')
            && end > 0
        {
            args = inner[..end]
                .trim()
                .split(',')
                .map(|a| a.trim().to_string())
                .filter(|a| !a.is_empty())
                .collect();
            tail = &inner[end + 1..];
        }
        let kind = match name {
            "all" => Kind::All,
            "hostile" => Kind::Hostile,
            "baby" => Kind::Baby,
            "tamed" => Kind::Tamed,
            "nametagged" => Kind::Nametagged,
            "silent" => Kind::Silent,
            "mob" => Kind::Mob,
            "leashed" => Kind::Leashed,
            "customdata" => Kind::CustomData,
            other => return Err(format!("Invalid tag: {other}")),
        };
        let single = Predicate::Single(kind, args);
        parts.push(if negated {
            Predicate::Not(Box::new(single))
        } else {
            single
        });
        rest = tail;
    }
    match parts.len() {
        0 => Err(format!("Invalid predicate(s): {line}")),
        1 => Ok(parts.remove(0)),
        _ => Ok(Predicate::All(parts)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tags_like_the_original() {
        assert!(matches!(
            parse_predicate("?hostile"),
            Ok(Predicate::Single(Kind::Hostile, _))
        ));
        assert!(matches!(
            parse_predicate("not ?hostile"),
            Ok(Predicate::Not(_))
        ));
        assert!(
            matches!(parse_predicate("?tamed ?nametagged(Dinner bone)"), Ok(Predicate::All(p)) if p.len() == 2)
        );
        assert!(parse_predicate("?bogus").is_err());
        let Ok(Predicate::Single(Kind::Mob, args)) = parse_predicate("?mob(creeper, zombie)")
        else {
            panic!()
        };
        assert_eq!(args, ["creeper", "zombie"]);
    }
}
