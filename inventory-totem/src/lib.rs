//! InventoryTotem: a totem of undying anywhere in the inventory saves a player from lethal damage.
//! Rust port of InventoryTotem 1.0.0 by Mobilestars (Apache-2.0); see README.md for the differences.

use pumpkin_plugin_api::{
    Context, EventHandler, ItemStack, Player, Plugin, PluginMetadata, Server,
    events::{EntityDamageEvent, EventPriority},
    events_wit::EntityDamageEventData,
    inventory_wit::PlayerInventory,
    particles::Particle,
    register_plugin,
    wit::pumpkin::plugin::status_effect::{StatusEffectInstance, StatusEffectType},
    world::{Sound, SoundCategory},
};

// The host returns registry keys without the "minecraft:" namespace.
const TOTEM: &str = "totem_of_undying";

struct InventoryTotem;

impl Plugin for InventoryTotem {
    fn new() -> Self {
        InventoryTotem
    }

    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "InventoryTotem".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            authors: vec!["Mobilestars".into()],
            // Same text as the original plugin.yml.
            description: "Makes the totem of undying works when it's anywhere in the inventory."
                .into(),
            dependencies: vec![],
            permissions: vec![],
        }
    }

    fn on_load(&self, context: Context) -> Result<(), String> {
        context.register_event_handler::<EntityDamageEvent, _>(
            TotemHandler,
            EventPriority::Normal,
            true,
        )?;
        Ok(())
    }
}

struct TotemHandler;

impl EventHandler<EntityDamageEvent> for TotemHandler {
    fn handle(&self, server: Server, mut event: EntityDamageEventData) -> EntityDamageEventData {
        let Some(player) = server
            .get_all_players()
            .into_iter()
            .find(|p| p.as_entity().get_id() as i32 == event.entity_id)
        else {
            return event;
        };
        if event.damage >= player.get_health() && take_totem(&player) {
            revive(&player);
            event.cancelled = true;
        }
        event
    }
}

type EquipmentSlot = (
    fn(&PlayerInventory) -> Option<ItemStack>,
    fn(&PlayerInventory, Option<ItemStack>),
);
const EQUIPMENT: [EquipmentSlot; 5] = [
    (|i| i.get_boots(), |i, s| i.set_boots(s)),
    (|i| i.get_leggings(), |i, s| i.set_leggings(s)),
    (|i| i.get_chestplate(), |i, s| i.set_chestplate(s)),
    (|i| i.get_helmet(), |i, s| i.set_helmet(s)),
    (|i| i.get_off_hand(), |i, s| i.set_off_hand(s)),
];

/// Removes one totem, searching in Bukkit's `PlayerInventory#getContents` order: storage, armor (boots first), offhand.
fn take_totem(player: &Player) -> bool {
    let inventory = player.get_inventory();
    let main = inventory.as_inventory();
    for slot in 0..main.get_size() {
        if let Some(stack) = main.get_item(slot).filter(is_totem) {
            main.set_item(slot, shrink(stack));
            return true;
        }
    }
    for (get, set) in EQUIPMENT {
        if let Some(stack) = get(&inventory).filter(is_totem) {
            set(&inventory, shrink(stack));
            return true;
        }
    }
    false
}

fn is_totem(stack: &ItemStack) -> bool {
    stack.get_registry_key() == TOTEM
}

fn shrink(stack: ItemStack) -> Option<ItemStack> {
    let count = stack.get_count();
    (count > 1).then(|| {
        stack.set_count(count - 1);
        stack
    })
}

/// Same effects, particles and sound as the original.
fn revive(player: &Player) {
    player.set_health(1.0);
    let entity = player.as_entity();
    entity.set_fire_ticks(0);
    entity.set_fall_distance(0.0);
    for (effect_type, duration, amplifier) in [
        (StatusEffectType::Regeneration, 40, 1),
        (StatusEffectType::Absorption, 100, 1),
        (StatusEffectType::FireResistance, 800, 0),
    ] {
        player.add_effect(StatusEffectInstance {
            effect_type,
            duration,
            amplifier,
            ambient: false,
            show_particles: true,
            show_icon: true,
        });
    }
    let world = player.get_world();
    let pos = player.get_position();
    world.spawn_particle(Particle::TotemOfUndying, pos, (0.5, 1.0, 0.5), 0.1, 30);
    world.play_sound(Sound::ItemTotemUse, SoundCategory::Master, pos, 1.0, 1.0);
}

register_plugin!(InventoryTotem);
