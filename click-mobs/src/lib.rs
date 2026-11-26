//! ClickMobs 1.3.1 by Clickism (GPL-3.0-only): sneak + right-click a mob to pick it up as a head, place it back.
//! Rust port; see README.md for the differences and HOST-API.md for what the host still lacks.

mod config;
mod entity_types;
mod lang;
mod mobs;
mod perm_mobs;
mod snapshot;
mod textures;

use config::{Config, OPTIONS, SETTABLE};
use entity_types::ALL;
use lang::{Kind, Lang, colorize};
use mobs::{MobList, Subject, key_of};
use pumpkin_plugin_api::{
    Context, Entity, EventHandler, ItemStack, Player, Plugin, PluginMetadata, Result, Server,
    command::{
        Arg, ArgumentType, Command, CommandError, CommandNode, CommandSender, CommandSuggestion,
        CommandSuggestions, ConsumedArgs, StringType, SuggestionRequest,
    },
    commands::{CommandHandler, CommandSuggestionHandler},
    common::{GameMode, Hand},
    data_components::DataComponent,
    events::{BlockPlaceEvent, EventPriority, PlayerInteractEntityEvent},
    events_wit::{BlockPlaceEventData, EntityInteractionAction, PlayerInteractEntityEventData},
    permission::{Permission, PermissionDefault, PermissionLevel},
    permissions::{FS_READ_DATA, FS_WRITE_DATA},
    persistent_data::{bool_tree, string_tree},
    register_plugin,
    scheduler::schedule_delayed_task,
    text::TextComponent,
    world::{Sound, SoundCategory},
};
use std::{
    collections::HashSet,
    sync::{Arc, RwLock},
};
use tracing::error;

const VERSION: &str = env!("CARGO_PKG_VERSION");

// ---- state ----

struct State {
    cfg: Config,
    white: MobList,
    black: MobList,
    items: HashSet<String>,
    lang: Lang,
    data_dir: String,
}

static STATE: RwLock<Option<Arc<State>>> = RwLock::new(None);

fn state() -> Arc<State> {
    STATE
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .expect("ClickMobs not loaded")
}

fn config_path(dir: &str) -> String {
    format!("{dir}/config.yml")
}

/// Reads config.yml (writing the defaults when it is missing or lacks keys, like the original) and the language file.
fn load(data_dir: &str) {
    let _ = std::fs::create_dir_all(data_dir);
    let path = config_path(data_dir);
    let (cfg, missing) = match std::fs::read_to_string(&path) {
        Ok(text) => Config::parse(&text),
        Err(_) => (Config::default(), true),
    };
    if missing && let Err(e) = std::fs::write(&path, cfg.render()) {
        error!("Failed to save config: {e}");
    }
    let state = State {
        white: MobList::parse(&cfg.whitelisted_mobs),
        black: MobList::parse(&cfg.blacklisted_mobs),
        items: cfg.blacklisted_items_in_hand.iter().cloned().collect(),
        lang: Lang::load(data_dir, &cfg.language),
        cfg,
        data_dir: data_dir.into(),
    };
    *STATE.write().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(state));
}

// ---- messages (original Message / MessageType) ----

fn legacy(text: &str) -> TextComponent {
    TextComponent::from_legacy_string(text)
}

fn play_sound(player: &Player, sound: Sound, volume: f32, pitch: f32) {
    player.play_sound(sound, SoundCategory::Master, volume, pitch);
}

fn kind_sound(player: &Player, kind: Kind) {
    match kind {
        Kind::Fail => play_sound(player, Sound::BlockAmethystBlockResonate, 1.0, 0.5),
        Kind::PickUp => {}
        Kind::Config => {
            play_sound(player, Sound::BlockNoteBlockChime, 1.0, 1.0);
            let name = player.get_name();
            schedule_delayed_task(2, move |server| {
                if let Some(p) = server.get_player_by_name(&name) {
                    play_sound(&p, Sound::BlockNoteBlockChime, 1.0, 2.0);
                }
            });
        }
    }
}

fn kind_of(key: &str) -> Option<Kind> {
    Some(match key {
        "update" | "usage" => return None,
        "no_permission" | "blacklisted_mob" | "write_error" | "read_error" => Kind::Fail,
        "pick_up" => Kind::PickUp,
        "config_set" | "config_get" | "config_path" | "config_reload" => Kind::Config,
        _ => return None,
    })
}

/// Chat message to a player: prefix + text, colourised, plus the type's sound unless `silent`.
fn chat(player: &Player, key: &str, params: &[(&str, &str)], silent: bool) {
    let st = state();
    let kind = kind_of(key).unwrap_or(Kind::Fail);
    let line = colorize(&format!("{}{}", kind.prefix(), st.lang.get(key, params)));
    player.send_system_message(legacy(&line), false);
    if !silent {
        kind_sound(player, kind);
    }
}

fn actionbar(player: &Player, key: &str, params: &[(&str, &str)], silent: bool) {
    let st = state();
    let kind = kind_of(key).unwrap_or(Kind::Fail);
    let line = kind
        .title_format()
        .replace("%s", &colorize(&st.lang.get(key, params)));
    player.show_actionbar(legacy(&colorize(&line)));
    if !silent {
        kind_sound(player, kind);
    }
}

fn send(sender: &CommandSender, key: &str, params: &[(&str, &str)]) {
    match sender.as_player() {
        Some(player) => chat(&player, key, params, false),
        None => {
            let st = state();
            let kind = kind_of(key).unwrap_or(Kind::Fail);
            sender.send_message(legacy(&colorize(&format!(
                "{}{}",
                kind.prefix(),
                st.lang.get(key, params)
            ))));
        }
    }
}

// ---- helpers ----

fn strip_ns(key: &str) -> &str {
    key.trim_start_matches("minecraft:")
}

fn capitalize(s: &str) -> String {
    s.split(' ')
        .filter(|w| !w.is_empty())
        .map(|w| w[..1].to_uppercase() + &w[1..])
        .collect::<Vec<_>>()
        .join(" ")
}

fn format_entity(key: &str, st: &State) -> String {
    let name = key.replace('_', " ");
    if st.lang.language() == "de_DE" {
        capitalize(&name[..1]) + &name[1..]
    } else {
        name
    }
}

fn display_name(sub: &Subject, st: &State) -> String {
    if let Some(name) = &sub.custom_name {
        return format!("\"{name}\"");
    }
    let name = capitalize(&format_entity(&sub.key, st));
    if sub.baby {
        st.lang.get("baby_mob", &[("mob", &name)])
    } else {
        name
    }
}

fn registry_key(item: &ItemStack) -> String {
    strip_ns(&item.get_registry_key()).to_string()
}

fn is_mob(item: &ItemStack) -> bool {
    registry_key(item) == "player_head" && item.has_custom_data(snapshot::NS, "entity")
}

fn find_entity(player: &Player, id: i32) -> Option<Entity> {
    let me = player.as_entity();
    let found = me
        .get_nearby_entities(8.0, 8.0, 8.0)
        .into_iter()
        .find(|e| e.get_id() as i32 == id);
    found.or_else(|| {
        player
            .get_world()
            .get_entities()
            .into_iter()
            .find(|e| e.get_id() as i32 == id)
    })
}

fn held_mob(player: &Player) -> Option<(ItemStack, Hand)> {
    let inv = player.get_inventory();
    [Hand::Right, Hand::Left].into_iter().find_map(|hand| {
        inv.get_item_in_hand(hand)
            .filter(is_mob)
            .map(|item| (item, hand))
    })
}

fn decrement(player: &Player, item: ItemStack, hand: Hand) {
    let count = item.get_count();
    let rest = (count > 1).then(|| {
        item.set_count(count - 1);
        item
    });
    player.get_inventory().set_item_in_hand(hand, rest);
}

/// Entity re-created from the mob item (original `spawnFromItemStack`).
fn spawn_from_item(
    item: &ItemStack,
    world: &pumpkin_plugin_api::world::World,
    pos: (f64, f64, f64),
) -> Option<Entity> {
    let ty = item.get_custom_data(snapshot::NS, "type")?;
    let ty = match ty.tags.get(ty.root as usize)? {
        pumpkin_plugin_api::common::NbtTag::StringTag(s) => s.to_lowercase(),
        _ => return None,
    };
    let entity_type = ALL.iter().find(|(k, _)| *k == ty)?.1;
    let nbt = match item
        .get_custom_data(snapshot::NS, "nbt")
        .and_then(|t| t.tags.get(t.root as usize).cloned())
    {
        Some(pumpkin_plugin_api::common::NbtTag::StringTag(s)) => s,
        _ => String::new(),
    };
    let entity = world.spawn_entity(entity_type, pos);
    let (name, age) = snapshot::read_nbt(&nbt);
    if let Some(name) = name {
        entity.set_custom_name(TextComponent::text(&name));
    }
    if age != 0
        && let Some(living) = entity.as_living()
    {
        living.set_age(age);
    }
    Some(entity)
}

// ---- pick up (original PickupManager#onInteract) ----

fn blacklisted_item_in_hand(st: &State, key: &str) -> bool {
    (st.items.contains("?all") && key != "air")
        || key.ends_with("_harness")
        || st.items.contains(key)
}

fn can_pick_up(st: &State, player: &Player, sub: &Subject) -> bool {
    if st.white.contains(sub) {
        return true;
    }
    let blacklisted = st.black.contains(sub);
    if st.cfg.per_mob_permissions {
        return player.has_permission(&format!("clickmobs.pickup.{}", sub.key)) && !blacklisted;
    }
    !blacklisted
}

fn pickup_effect(entity: &Entity) {
    let (x, y, z) = entity.get_position();
    let pos = (x, y + 0.25, z);
    let world = entity.get_world();
    world.spawn_particle(
        pumpkin_plugin_api::particles::Particle::SweepAttack,
        pos,
        (0.0, 0.0, 0.0),
        0.0,
        1,
    );
    world.play_sound(
        Sound::EntityPlayerAttackSweep,
        SoundCategory::Master,
        pos,
        1.0,
        0.5,
    );
}

/// Slot for the new item: the held slot when empty, else the first free inventory slot (the original drops the
/// leftover; the host has no drop-item yet, so a full inventory refuses the pick up).
fn free_slot(player: &Player) -> Option<Option<u32>> {
    let inv = player.get_inventory();
    if inv.get_item_in_hand(Hand::Right).is_none() {
        return Some(None);
    }
    let main = inv.as_inventory();
    (0..main.get_size())
        .find(|s| main.get_item(*s).is_none())
        .map(Some)
}

fn pick_up(server: &Server, data: &mut PlayerInteractEntityEventData) {
    let _ = server;
    let st = state();
    let player = &data.player;
    let Some(entity) = find_entity(player, data.entity_id) else {
        return;
    };
    if entity.as_living().is_none() {
        return;
    }
    let sub = Subject::new(entity);
    if sub.key == "player" || player.get_gamemode() == GameMode::Spectator {
        return;
    }
    let held = player
        .get_inventory()
        .get_item_in_hand(Hand::Right)
        .map_or_else(|| "air".into(), |i| registry_key(&i));
    if blacklisted_item_in_hand(&st, &held) {
        return;
    }
    if !can_pick_up(&st, player, &sub) {
        actionbar(player, "blacklisted_mob", &[], false);
        return;
    }
    if !player.has_permission("clickmobs.pickup") {
        chat(player, "no_permission", &[], false);
        return;
    }
    data.cancelled = true;

    let mut root = sub.entity.get_vehicle().filter(|v| v.as_living().is_some());
    while let Some(parent) = root
        .as_ref()
        .and_then(|r| r.get_vehicle())
        .filter(|v| v.as_living().is_some())
    {
        root = Some(parent);
    }
    let root = root.map(Subject::new);
    let root = root.as_ref().unwrap_or(&sub);
    let has_passengers = root
        .entity
        .get_passengers()
        .iter()
        .any(|p| p.as_living().is_some() && key_of(p) != "player");
    let slot = free_slot(player);
    if has_passengers || slot.is_none() {
        // shortcut: passengers cannot be stored without entity NBT (HOST-API.md 1); refuse instead of losing them.
        chat(player, "write_error", &[], false);
        error!(
            "Failed to write mob data: {}",
            if has_passengers {
                "mob has passengers"
            } else {
                "inventory is full"
            }
        );
        return;
    }
    // Players riding the mob are dropped off, like leaveVehicle().
    for p in root.entity.get_passengers() {
        if key_of(&p) == "player" {
            root.entity.remove_passenger(p);
        }
    }

    let age = root.entity.as_living().map_or(0, |l| l.get_age());
    let item = create_item(&st, root, age);
    root.entity.remove();
    match slot.flatten() {
        Some(index) => player
            .get_inventory()
            .as_inventory()
            .set_item(index, Some(item)),
        None => player
            .get_inventory()
            .set_item_in_hand(Hand::Right, Some(item)),
    }
    actionbar(
        player,
        "pick_up",
        &[("mob", &format_entity(&sub.key, &st))],
        true,
    );
    pickup_effect(&sub.entity);
}

fn create_item(st: &State, sub: &Subject, age: i32) -> ItemStack {
    let item = ItemStack::new("player_head", 1);
    item.set_custom_name(Some(legacy(&format!("§e{}", display_name(sub, st)))));
    let lore = st
        .lang
        .get("mob.lore", &[("mob", &format_entity(&sub.key, st))]);
    item.set_lore(lore.split('\n').map(legacy).collect::<Vec<_>>());
    let model = st.cfg.custom_model_data.get(&sub.key).copied().unwrap_or(0);
    if model != 0 {
        item.set_component(
            DataComponent::CustomModelData,
            &snapshot::model_data_component(model),
        );
    }
    let seed = (u128::from(sub.entity.get_id()) << 64)
        ^ u128::from(sub.entity.get_ticks_lived() as u32)
        ^ 0x4000_0000_0000_8000_0000_0000_0000_0000;
    item.set_component(
        DataComponent::Profile,
        &snapshot::profile_component(&sub.key, seed),
    );
    item.set_custom_data(snapshot::NS, "entity", &bool_tree(true));
    item.set_custom_data(snapshot::NS, "type", &string_tree(&sub.key.to_uppercase()));
    item.set_custom_data(
        snapshot::NS,
        "nbt",
        &string_tree(&snapshot::write_nbt(
            &sub.key,
            sub.custom_name.as_deref(),
            age,
        )),
    );
    item
}

// ---- vehicles (original VehicleInteractListener) ----

fn vehicle_capacity(key: &str) -> Option<usize> {
    if key == "minecart" || key.ends_with("_chest_boat") || key.ends_with("_chest_raft") {
        Some(1)
    } else if key.ends_with("_boat") || key.ends_with("_raft") {
        Some(2)
    } else {
        None
    }
}

fn is_vehicle(key: &str) -> bool {
    key.ends_with("minecart") || key.ends_with("_boat") || key.ends_with("_raft")
}

/// Returns true when the interaction was handled.
fn put_in_vehicle(data: &mut PlayerInteractEntityEventData) -> bool {
    let player = &data.player;
    let Some((item, hand)) = player
        .get_inventory()
        .get_item_in_hand(Hand::Right)
        .filter(is_mob)
        .map(|i| (i, Hand::Right))
    else {
        return false;
    };
    let Some(vehicle) = find_entity(player, data.entity_id) else {
        return false;
    };
    let key = key_of(&vehicle);
    if !is_vehicle(&key) {
        return false;
    }
    data.cancelled = true;
    if vehicle_capacity(&key).is_none_or(|cap| vehicle.get_passengers().len() >= cap) {
        return true;
    }
    let world = player.get_world();
    match spawn_from_item(&item, &world, vehicle.get_position()) {
        Some(mob) => {
            decrement(player, item, hand);
            let pos = player.get_position();
            if key.ends_with("minecart") {
                world.play_sound(Sound::BlockMetalBreak, SoundCategory::Master, pos, 1.0, 0.5);
            } else {
                world.play_sound(Sound::BlockWoodBreak, SoundCategory::Master, pos, 1.0, 0.5);
            }
            vehicle.add_passenger(mob);
        }
        None => {
            chat(player, "read_error", &[], false);
            error!("Failed to read mob data: item has no usable entity data");
        }
    }
    true
}

struct InteractHandler;

impl EventHandler<PlayerInteractEntityEvent> for InteractHandler {
    fn handle(
        &self,
        server: Server,
        mut data: PlayerInteractEntityEventData,
    ) -> PlayerInteractEntityEventData {
        // shortcut: the event has no hand; right-click interaction in the main hand is assumed.
        if data.cancelled || data.action != EntityInteractionAction::Interact || !data.sneaking {
            return data;
        }
        if !put_in_vehicle(&mut data) {
            pick_up(&server, &mut data);
        }
        data
    }
}

// ---- place (original PickupManager#onPlace) ----

struct PlaceHandler;

impl EventHandler<BlockPlaceEvent> for PlaceHandler {
    fn handle(&self, _server: Server, mut data: BlockPlaceEventData) -> BlockPlaceEventData {
        if data.cancelled {
            return data;
        }
        let player = &data.player;
        let Some((item, hand)) = held_mob(player) else {
            return data;
        };
        data.cancelled = true;
        if !player.has_permission("clickmobs.place") {
            chat(player, "no_permission", &[], false);
            return data;
        }
        let pos = (
            f64::from(data.block_pos.x) + 0.5,
            f64::from(data.block_pos.y),
            f64::from(data.block_pos.z) + 0.5,
        );
        let world = player.get_world();
        match spawn_from_item(&item, &world, pos) {
            Some(mob) => {
                // Face the entity towards the player.
                let yaw = player.as_entity().get_yaw();
                mob.set_rotation((yaw + 360.0) % 360.0 - 180.0, 0.0);
                decrement(player, item, hand);
                world.play_sound(
                    Sound::EntityPlayerAttackWeak,
                    SoundCategory::Master,
                    pos,
                    1.0,
                    0.5,
                );
            }
            None => {
                chat(player, "read_error", &[], false);
                error!("Failed to read mob data: item has no usable entity data");
            }
        }
        data
    }
}

// ---- /clickmobs (original Configured PaperCommandAdapter) ----

struct Reload;
impl CommandHandler for Reload {
    fn handle(
        &self,
        sender: CommandSender,
        _s: Server,
        _a: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        load(&state().data_dir);
        send(&sender, "config_reload", &[]);
        Ok(1)
    }
}

struct Path;
impl CommandHandler for Path {
    fn handle(
        &self,
        sender: CommandSender,
        _s: Server,
        _a: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let file = config_path(&state().data_dir);
        let abs = std::fs::canonicalize(&file).map_or(file, |p| p.display().to_string());
        send(&sender, "config_path", &[("path", &abs)]);
        Ok(1)
    }
}

struct Get;
impl CommandHandler for Get {
    fn handle(
        &self,
        sender: CommandSender,
        _s: Server,
        args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let Arg::Simple(option) = args.get_value("option") else {
            return Ok(0);
        };
        let Some(value) = state().cfg.display(&option) else {
            return Err(CommandError::CommandFailed(TextComponent::text(&format!(
                "Expected literal {option}"
            ))));
        };
        send(
            &sender,
            "config_get",
            &[("option", &option), ("value", &value)],
        );
        Ok(1)
    }
}

struct Set(&'static str);
impl CommandHandler for Set {
    fn handle(
        &self,
        sender: CommandSender,
        _s: Server,
        args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let st = state();
        let mut cfg = st.cfg.clone();
        let shown = match (self.0, args.get_value("value")) {
            ("language", Arg::Simple(v) | Arg::Msg(v)) => {
                cfg.language = v.clone();
                v
            }
            ("check_update", Arg::Bool(v)) => {
                cfg.check_update = v;
                v.to_string()
            }
            ("per_mob_permissions", Arg::Bool(v)) => {
                cfg.per_mob_permissions = v;
                v.to_string()
            }
            ("enable_dispensers", Arg::Bool(v)) => {
                cfg.enable_dispensers = v;
                v.to_string()
            }
            _ => return Ok(0),
        };
        if let Err(e) = std::fs::write(config_path(&st.data_dir), cfg.render()) {
            error!("Failed to save config: {e}");
        }
        load(&st.data_dir);
        send(
            &sender,
            "config_set",
            &[("option", self.0), ("value", &shown)],
        );
        Ok(1)
    }
}

struct OptionNames;
impl CommandSuggestionHandler for OptionNames {
    fn suggest(
        &self,
        _s: CommandSender,
        _srv: Server,
        request: SuggestionRequest,
    ) -> CommandSuggestions {
        let start = request.start as usize;
        let typed = request
            .input
            .get(start..)
            .unwrap_or_default()
            .to_lowercase();
        CommandSuggestions {
            start: request.start,
            length: (request.input.len() - start) as u32,
            values: OPTIONS
                .iter()
                .filter(|o| o.starts_with(&typed))
                .map(|o| CommandSuggestion {
                    value: (*o).into(),
                    tooltip: None,
                })
                .collect(),
        }
    }
}

fn build_command() -> Command {
    let set = SETTABLE
        .iter()
        .fold(CommandNode::literal("set"), |set, option| {
            let ty = if *option == "language" {
                ArgumentType::String(StringType::Quotable)
            } else {
                ArgumentType::Bool
            };
            set.then(
                CommandNode::literal(option)
                    .then(CommandNode::argument("value", &ty).execute(Set(option))),
            )
        });
    let get = CommandNode::literal("get").then(
        CommandNode::argument("option", &ArgumentType::String(StringType::SingleWord))
            .suggest(OptionNames)
            .execute(Get),
    );
    // The original's PaperCommandAdapter.buildRoot() puts every option command under a `config` literal.
    Command::new(&["clickmobs".into()], "Main command for ClickMobs.").then(
        CommandNode::literal("config")
            .then(CommandNode::literal("reload").execute(Reload))
            .then(CommandNode::literal("path").execute(Path))
            .then(get)
            .then(set),
    )
}

// ---- plugin ----

struct ClickMobs;

impl Plugin for ClickMobs {
    fn new() -> Self {
        ClickMobs
    }

    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "ClickMobs".into(),
            version: VERSION.into(),
            authors: vec!["Clickism".into()],
            // Same text as the original plugin.yml.
            description: "A simple plugin that allows you to pick up any mob.".into(),
            dependencies: vec![],
            permissions: vec![FS_READ_DATA.into(), FS_WRITE_DATA.into()],
        }
    }

    fn on_load(&self, context: Context) -> Result<()> {
        load(&context.get_data_folder());
        let perm = |node: String, description: &str, default| Permission {
            node,
            description: description.into(),
            default,
            children: vec![],
        };
        let mut permissions = vec![
            perm(
                "clickmobs.pickup".into(),
                "Allows players to pick up mobs.",
                PermissionDefault::Allow,
            ),
            perm(
                "clickmobs.place".into(),
                "Allows players to place mobs.",
                PermissionDefault::Allow,
            ),
            perm(
                "clickmobs.config".into(),
                "Allows players to configure and reload the plugin via commands.",
                PermissionDefault::Op(PermissionLevel::One),
            ),
        ];
        for action in ["pickup", "place"] {
            for mob in perm_mobs::MOBS {
                permissions.push(perm(
                    format!("clickmobs.{action}.{mob}"),
                    "",
                    PermissionDefault::Deny,
                ));
            }
        }
        for p in &permissions {
            context.register_permission(p)?;
        }
        context.register_event_handler::<PlayerInteractEntityEvent, _>(
            InteractHandler,
            EventPriority::High,
            true,
        )?;
        context.register_event_handler::<BlockPlaceEvent, _>(
            PlaceHandler,
            EventPriority::High,
            true,
        )?;
        context.register_command(build_command(), "clickmobs.config");
        Ok(())
    }
}

register_plugin!(ClickMobs);
