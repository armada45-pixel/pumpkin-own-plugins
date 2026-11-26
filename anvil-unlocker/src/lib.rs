//! AnvilUnlocker: lifts the anvil's "Too Expensive!" cap. Rust port of AnvilUnlocker 1.1.3 (Unlicense).
//! The cap itself needs host API Pumpkin lacks, see HOST-API.md and README.md.

use std::sync::atomic::{AtomicI32, Ordering};

use pumpkin_plugin_api::{
    Context, EventHandler, Player, Plugin, PluginMetadata, Screen, Server,
    common::GameMode,
    events::{EventPriority, InventoryCloseEvent, PrepareAnvilEvent},
    events_wit::{InventoryCloseEventData, PrepareAnvilEventData},
    java_packets::{CPlayerAbilities, ClientboundPacket},
    permissions::{FS_READ_DATA, FS_WRITE_DATA},
    register_plugin,
};

/// Same default file the original ships (`saveDefaultConfig`).
const DEFAULT_CONFIG: &str = "maximumCost: 32767\n";
const CONFIG: &str = "data/config.yml";
/// Bukkit's `Short.MAX_VALUE`.
const CEILING: i32 = i16::MAX as i32;

/// Effective `maximumCost`. No code applies it to the anvil yet (HOST-API.md item 1).
static MAXIMUM_COST: AtomicI32 = AtomicI32::new(CEILING);

fn constrain_anvil_max(actual: i32) -> i32 {
    actual.clamp(41, CEILING)
}

/// `getConfig().getInt("maximumCost")`: a missing or non-integer value reads as 0.
// shortcut: line scan instead of a YAML parser, covers `maximumCost: <int>` (also quoted); upgrade if keys grow.
fn read_maximum_cost(yaml: &str) -> i32 {
    yaml.lines()
        .find_map(|l| l.strip_prefix("maximumCost:"))
        .and_then(|v| {
            v.split('#')
                .next()?
                .trim()
                .trim_matches(['"', '\''])
                .parse()
                .ok()
        })
        .unwrap_or(0)
}

struct AnvilUnlocker;

impl Plugin for AnvilUnlocker {
    fn new() -> Self {
        AnvilUnlocker
    }

    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "AnvilUnlocker".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            authors: vec!["Jikoo".into(), "Cloudnode".into()],
            description: "Anvil Unlocker".into(),
            dependencies: vec![],
            permissions: vec![FS_READ_DATA.into(), FS_WRITE_DATA.into()],
        }
    }

    fn on_load(&self, context: Context) -> Result<(), String> {
        if !std::path::Path::new(CONFIG).exists() {
            std::fs::write(CONFIG, DEFAULT_CONFIG)
                .map_err(|e| format!("cannot save {CONFIG}: {e}"))?;
        }
        let yaml =
            std::fs::read_to_string(CONFIG).map_err(|e| format!("cannot read {CONFIG}: {e}"))?;
        MAXIMUM_COST.store(
            constrain_anvil_max(read_maximum_cost(&yaml)),
            Ordering::Relaxed,
        );

        context.register_event_handler::<InventoryCloseEvent, _>(
            CloseHandler,
            EventPriority::Normal,
            true,
        )?;
        context.register_event_handler::<PrepareAnvilEvent, _>(
            PrepareHandler,
            EventPriority::Normal,
            true,
        )?;
        Ok(())
    }
}

/// The original's `setInstantBuild`: a player-abilities packet that keeps everything else as is.
fn set_instant_build(player: &Player, instant_build: bool) {
    let a = player.get_abilities();
    let flags = u8::from(a.invulnerable)
        | u8::from(a.flying) << 1
        | u8::from(a.allow_flying) << 2
        | u8::from(instant_build) << 3;
    // The host's speeds are already in packet units (Bukkit's getFlySpeed()/2).
    if let Some(java) = player.as_java() {
        java.send_packet(&ClientboundPacket::CPlayerAbilities(CPlayerAbilities {
            flags,
            flying_speed: a.fly_speed,
            field_of_view: a.walk_speed,
        }));
    }
}

struct CloseHandler;

impl EventHandler<InventoryCloseEvent> for CloseHandler {
    fn handle(&self, _server: Server, event: InventoryCloseEventData) -> InventoryCloseEventData {
        if event.window_type == Some(Screen::Anvil)
            && event.player.get_gamemode() != GameMode::Creative
        {
            set_instant_build(&event.player, false);
        }
        event
    }
}

struct PrepareHandler;

impl EventHandler<PrepareAnvilEvent> for PrepareHandler {
    fn handle(&self, _server: Server, event: PrepareAnvilEventData) -> PrepareAnvilEventData {
        if event.player.get_gamemode() != GameMode::Creative {
            // Original: setMaximumRepairCost(max) on the anvil, then instant build unless the cost reached the
            // maximum. Neither the anvil nor its second slot is reachable yet, so this only uses the event's cost.
            let below_max =
                i64::from(event.repair_cost) < i64::from(MAXIMUM_COST.load(Ordering::Relaxed));
            set_instant_build(&event.player, below_max);
        }
        event
    }
}

register_plugin!(AnvilUnlocker);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_parsing_matches_bukkit() {
        assert_eq!(read_maximum_cost(DEFAULT_CONFIG), 32767);
        assert_eq!(read_maximum_cost("maximumCost: 100 # x\n"), 100);
        assert_eq!(read_maximum_cost("maximumCost: abc\n"), 0);
        assert_eq!(read_maximum_cost(""), 0);
        assert_eq!(constrain_anvil_max(0), 41);
        assert_eq!(constrain_anvil_max(99999), 32767);
        assert_eq!(constrain_anvil_max(500), 500);
    }
}
