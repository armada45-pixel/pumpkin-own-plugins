//! craftableinvframes 2.4.2: invisible item frames for survival players.
//! Rust port of CraftableInvFrames (MIT for Techdoodle's code, GPL-3.0-or-later for the fork's additions);
//! see README.md for what works differently and HOST-API.md for what the host cannot do yet.

use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};

use pumpkin_plugin_api::{
    Context, EventHandler, ItemStack, Player, Plugin, PluginMetadata, Server,
    command::{
        Arg, ArgumentType, Command, CommandError, CommandNode, CommandSender, CommandSuggestion,
        CommandSuggestions, ConsumedArgs, StringType, SuggestionRequest,
    },
    commands::{CommandHandler, CommandSuggestionHandler},
    events::{EventPriority, HangingPlaceEvent, PrepareItemCraftEvent},
    events_wit::{HangingPlaceEventData, PrepareItemCraftEventData},
    permissions,
    recipe::ShapedRecipeBuilder,
    register_plugin,
    text::TextComponent,
    wit::pumpkin::plugin::{
        common::Hand,
        entity_types::EntityType,
        permission::{Permission, PermissionDefault, PermissionLevel},
    },
};

const DEFAULT_CONFIG: &str = include_str!("default-config.yml");
const NS: &str = "craftableinvframes";
const KEY: &str = "invisible";
const RECIPE_ID: &str = "craftableinvframes:invisible-recipe";
const PERMS: [(&str, bool); 8] = [
    ("craftableinvframes.place", true),
    ("craftableinvframes.craft", true),
    ("craftableinvframes.cmd", false),
    ("craftableinvframes.reload", false),
    ("craftableinvframes.forcerecheck", false),
    ("craftableinvframes.get", false),
    ("craftableinvframes.give", false),
    ("craftableinvframes.setitem", false),
];

static CONFIG: Mutex<Option<Config>> = Mutex::new(None);
static DATA_DIR: Mutex<String> = Mutex::new(String::new());
static FRAMES_GLOW: AtomicBool = AtomicBool::new(false);
static FIRST_LOAD: AtomicBool = AtomicBool::new(true);

/// The user's config.yml over the shipped defaults (missing keys fall back, like Bukkit's copyDefaults).
struct Config {
    user: serde_yaml::Value,
    defaults: serde_yaml::Value,
}

fn lookup<'a>(v: &'a serde_yaml::Value, path: &str) -> Option<&'a serde_yaml::Value> {
    path.split('.').try_fold(v, |v, k| v.get(k))
}

fn cfg_get<T>(path: &str, pick: impl Fn(&serde_yaml::Value) -> Option<T>) -> Option<T> {
    let guard = CONFIG.lock().unwrap_or_else(|e| e.into_inner());
    let c = guard.as_ref()?;
    lookup(&c.user, path)
        .and_then(&pick)
        .or_else(|| lookup(&c.defaults, path).and_then(&pick))
}

fn cfg_str(path: &str) -> String {
    cfg_get(path, |v| v.as_str().map(str::to_owned)).unwrap_or_default()
}

fn config_path() -> String {
    format!(
        "{}/config.yml",
        DATA_DIR.lock().unwrap_or_else(|e| e.into_inner())
    )
}

fn load_config() {
    let path = config_path();
    if !std::path::Path::new(&path).exists() {
        let _ = std::fs::write(&path, DEFAULT_CONFIG); // saveDefaultConfig()
    }
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| DEFAULT_CONFIG.into());
    let defaults = serde_yaml::from_str(DEFAULT_CONFIG).expect("shipped config is valid");
    let user = serde_yaml::from_str(&text).unwrap_or(serde_yaml::Value::Null);
    *CONFIG.lock().unwrap_or_else(|e| e.into_inner()) = Some(Config { user, defaults });
}

fn legacy(text: &str) -> TextComponent {
    TextComponent::from_legacy_string(text)
}

fn tell(sender: &CommandSender, key: &str) {
    sender.send_message(legacy(&cfg_str(&format!("messages.{key}"))));
}

fn frame_item(glow: bool) -> ItemStack {
    let (id, name) = if glow {
        ("minecraft:glow_item_frame", "itemname-glow-invframe")
    } else {
        ("minecraft:item_frame", "itemname-invframe")
    };
    let stack = ItemStack::new(id, 1);
    stack.set_custom_name(Some(legacy(&format!("§f{}", cfg_str(name)))));
    stack.set_custom_data(NS, KEY, &pumpkin_plugin_api::persistent_data::byte_tree(1));
    stack
}

fn is_frame_type(t: EntityType) -> bool {
    matches!(t, EntityType::ItemFrame | EntityType::GlowItemFrame)
}

fn is_invisible_frame(e: &pumpkin_plugin_api::world::Entity) -> bool {
    is_frame_type(e.get_type()) && e.has_custom_data(NS, KEY)
}

/// Same effect as ItemFrame#setVisible + setGlowing in the original (visible == not invisible).
fn style_frame(e: &pumpkin_plugin_api::world::Entity, shown: bool) {
    e.set_invisible(!shown);
    e.set_glowing(shown);
}

fn force_recheck(server: &Server) {
    // shortcut: item-in-frame is not readable, so with glow on empty and filled frames cannot be told apart.
    if FRAMES_GLOW.load(Ordering::Relaxed) {
        return;
    }
    for world in server.get_all_worlds() {
        for e in world.get_entities().into_iter().filter(is_invisible_frame) {
            style_frame(&e, false);
        }
    }
}

fn recipe_ingredient() -> String {
    let t = cfg_get("recipe.type", |v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "LINGERING_POTION".into());
    format!("minecraft:{}", t.to_lowercase())
}

fn reload(server: &Server) {
    load_config();
    let glow = cfg_get("item-frames-glow", |v| v.as_bool()).unwrap_or(true);
    if FIRST_LOAD.swap(false, Ordering::Relaxed) || glow != FRAMES_GLOW.load(Ordering::Relaxed) {
        FRAMES_GLOW.store(glow, Ordering::Relaxed);
        force_recheck(server);
    }
    let output = frame_item(false);
    output.set_count(8);
    // Re-registering an existing id may be refused by the host; the old recipe then stays until restart.
    let _ = ShapedRecipeBuilder::new(RECIPE_ID, output)
        .pattern(["FFF", "FPF", "FFF"])
        .key('F', "minecraft:item_frame")
        .key('P', recipe_ingredient())
        .register(&server.get_recipe_manager());
}

fn first_empty_slot(player: &Player) -> Option<u32> {
    let inv = player.get_inventory().as_inventory();
    (0..inv.get_size()).find(|&s| inv.get_item(s).is_none())
}

/// giveItem(): the permission is checked on the receiver (the original passes the target as the sender).
fn give_item(player: &Player, server: &Server) {
    let msg =
        |key: &str| player.send_system_message(legacy(&cfg_str(&format!("messages.{key}"))), false);
    let _ = server;
    if !player.has_permission("craftableinvframes.get") {
        return msg("no-access");
    }
    if let Some(slot) = first_empty_slot(player) {
        player
            .get_inventory()
            .as_inventory()
            .set_item(slot, Some(frame_item(false)));
    }
    msg("give-success");
}

fn set_recipe_item(sender: &CommandSender, player: &Player, server: &Server) {
    let held = player.get_inventory().get_item_in_hand(Hand::Right);
    let ty = held
        .map(|s| {
            s.get_registry_key()
                .trim_start_matches("minecraft:")
                .to_uppercase()
        })
        .unwrap_or_else(|| "AIR".into());
    // Rewrite only the `recipe:` block so comments and the other keys stay as they are.
    let path = config_path();
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| DEFAULT_CONFIG.into());
    let block = format!("recipe:\n  ==: org.bukkit.inventory.ItemStack\n  v: 2567\n  type: {ty}\n");
    let mut out = String::new();
    let mut skipping = false;
    let mut done = false;
    for line in text.split_inclusive('\n') {
        if line.starts_with("recipe:") {
            skipping = true;
            done = true;
            out.push_str(&block);
        } else if skipping && (line.starts_with(' ') || line.trim().is_empty()) {
            continue;
        } else {
            skipping = false;
            out.push_str(line);
        }
    }
    if !done {
        out.push_str(&block);
    }
    let _ = std::fs::write(&path, out);
    reload(server);
    tell(sender, "recipe-update-success");
}

struct Iframe;

impl CommandHandler for Iframe {
    fn handle(
        &self,
        sender: CommandSender,
        server: Server,
        args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let line = match args.get_value("args") {
            Arg::Simple(s) | Arg::Msg(s) => s,
            _ => String::new(),
        };
        let parts: Vec<&str> = line.split_whitespace().collect();
        let sub = parts.first().map(|s| s.to_lowercase());
        let allowed = |node: &str| sender.has_permission(&server, node);
        match sub.as_deref() {
            None | Some("get") => match sender.as_player() {
                _ if !allowed("craftableinvframes.get") => tell(&sender, "no-access"),
                None => tell(&sender, "not-player"),
                Some(p) => give_item(&p, &server),
            },
            Some("give") => match parts.get(1) {
                None => tell(&sender, "specify-player-name"),
                Some(name) => match server.get_player_by_name(name) {
                    None => sender.send_message(legacy(
                        &cfg_str("messages.player-not-found").replace("%player%", name),
                    )),
                    Some(target) => give_item(&target, &server),
                },
            },
            Some("reload") if !allowed("craftableinvframes.reload") => tell(&sender, "no-access"),
            Some("reload") => {
                reload(&server);
                tell(&sender, "reload-success");
            }
            Some("force-recheck") if !allowed("craftableinvframes.forcerecheck") => {
                tell(&sender, "no-access")
            }
            Some("force-recheck") => {
                force_recheck(&server);
                tell(&sender, "recheck-success");
            }
            Some("setitem") if !allowed("craftableinvframes.setitem") => tell(&sender, "no-access"),
            Some("setitem") => match sender.as_player() {
                None => tell(&sender, "not-player"),
                Some(p) => set_recipe_item(&sender, &p, &server),
            },
            // The original returns false, so Bukkit prints the plugin.yml usage.
            Some(_) => sender.send_message(TextComponent::text("/iframe")),
        }
        Ok(1)
    }
}

struct Suggest;

impl CommandSuggestionHandler for Suggest {
    fn suggest(
        &self,
        sender: CommandSender,
        server: Server,
        request: SuggestionRequest,
    ) -> CommandSuggestions {
        let first = !request.remaining.contains(' ');
        let prefix = request.remaining.to_lowercase();
        let mut values: Vec<String> = if first {
            [
                ("get", "get"),
                ("give", "give"),
                ("reload", "reload"),
                ("force-recheck", "forcerecheck"),
                ("setitem", "setitem"),
            ]
            .into_iter()
            .filter(|(_, p)| sender.has_permission(&server, &format!("craftableinvframes.{p}")))
            .map(|(n, _)| n.to_string())
            .filter(|n| n.starts_with(&prefix))
            .collect()
        } else {
            Vec::new()
        };
        values.sort();
        CommandSuggestions {
            start: request.start,
            length: request.remaining.len() as u32,
            values: values
                .into_iter()
                .map(|value| CommandSuggestion {
                    value,
                    tooltip: None,
                })
                .collect(),
        }
    }
}

struct PrepareCraft;

impl EventHandler<PrepareItemCraftEvent> for PrepareCraft {
    fn handle(&self, _: Server, mut e: PrepareItemCraftEventData) -> PrepareItemCraftEventData {
        if !e.cancelled
            && e.recipe_id == RECIPE_ID
            && !e.player.has_permission("craftableinvframes.craft")
        {
            e.cancelled = true;
        }
        e
    }
}

struct HangingPlace;

impl EventHandler<HangingPlaceEvent> for HangingPlace {
    fn handle(&self, _: Server, mut e: HangingPlaceEventData) -> HangingPlaceEventData {
        if !e.cancelled
            && e.player
                .as_ref()
                .is_some_and(|p| place(p, e.entity_id as u32))
        {
            e.cancelled = true;
        }
        e
    }
}

/// Marks a freshly placed invisible frame; returns true when the placement must be cancelled.
fn place(p: &Player, entity_id: u32) -> bool {
    let inv = p.get_inventory();
    let held = [Hand::Right, Hand::Left]
        .into_iter()
        .filter_map(|h| inv.get_item_in_hand(h))
        .find(|s| {
            matches!(
                s.get_registry_key().trim_start_matches("minecraft:"),
                "item_frame" | "glow_item_frame"
            )
        });
    if !held.is_some_and(|s| s.has_custom_data(NS, KEY)) {
        return false;
    }
    if !p.has_permission("craftableinvframes.place") {
        return true;
    }
    let frames = p.get_world().get_entities();
    if let Some(frame) = frames
        .into_iter()
        .find(|x| x.get_id() == entity_id && is_frame_type(x.get_type()))
    {
        // A fresh frame is empty: glow mode shows it glowing, otherwise it is invisible.
        style_frame(&frame, FRAMES_GLOW.load(Ordering::Relaxed));
        frame.set_custom_data(NS, KEY, &pumpkin_plugin_api::persistent_data::byte_tree(1));
    }
    false
}

struct CraftableInvFrames;

impl Plugin for CraftableInvFrames {
    fn new() -> Self {
        CraftableInvFrames
    }

    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "craftableinvframes".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            authors: vec!["Techdoodle".into(), "SpiritOTHawk".into()],
            description:
                "This plugin enables the use of invisible item frames for survival players".into(),
            dependencies: vec![],
            permissions: vec![
                permissions::FS_READ_DATA.into(),
                permissions::FS_WRITE_DATA.into(),
            ],
        }
    }

    fn on_load(&self, context: Context) -> Result<(), String> {
        *DATA_DIR.lock().unwrap_or_else(|e| e.into_inner()) = context.get_data_folder();
        for (node, allow) in PERMS {
            context.register_permission(&Permission {
                node: node.into(),
                description: String::new(),
                default: if allow {
                    PermissionDefault::Allow
                } else {
                    PermissionDefault::Op(PermissionLevel::One)
                },
                children: vec![],
            })?;
        }
        let server = context.get_server();
        reload(&server);
        context.register_event_handler::<HangingPlaceEvent, _>(
            HangingPlace,
            EventPriority::Highest,
            true,
        )?;
        context.register_event_handler::<PrepareItemCraftEvent, _>(
            PrepareCraft,
            EventPriority::Normal,
            true,
        )?;
        let args = CommandNode::argument("args", &ArgumentType::String(StringType::Greedy))
            .suggest(Suggest)
            .execute(Iframe);
        context.register_command(
            Command::new(
                &["iframe".to_string()],
                "Gives the player an invisible item frame",
            )
            .then(args)
            .execute(Iframe),
            "craftableinvframes.cmd",
        );
        Ok(())
    }
}

register_plugin!(CraftableInvFrames);
