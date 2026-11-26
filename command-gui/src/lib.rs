//! CommandGUI: a configurable inventory menu that runs commands.
//! Rust port of CommandGUI 3.3.0 by Alfie51m (Apache-2.0); see README.md for the differences.

use pumpkin_plugin_api::{
    Context, EventHandler, Item, ItemStack, Player, Plugin, PluginMetadata, Screen, Server,
    command::{
        ArgumentType, Command, CommandError, CommandNode, CommandSender, CommandSuggestion,
        CommandSuggestions, ConsumedArgs, StringType, SuggestionRequest,
    },
    commands::{CommandHandler, CommandSuggestionHandler},
    common::Hand,
    events::{EventPriority, InventoryClickEvent, InventoryCloseEvent, PlayerInteractEvent},
    events_wit::{InventoryClickEventData, InventoryCloseEventData, PlayerInteractEventData},
    gui::Gui,
    permission::{Permission, PermissionDefault, PermissionLevel},
    permissions, register_plugin,
    server::CommandSender as ServerSender,
    text::TextComponent,
};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::Path,
    sync::{Arc, Mutex, RwLock},
    time::{SystemTime, UNIX_EPOCH},
};

const DATA: &str = "data";
const LANG_FILES: [(&str, &str); 8] = [
    ("en_us.yml", include_str!("../resources/lang/en_us.yml")),
    ("es_es.yml", include_str!("../resources/lang/es_es.yml")),
    ("de_de.yml", include_str!("../resources/lang/de_de.yml")),
    ("fr_fr.yml", include_str!("../resources/lang/fr_fr.yml")),
    ("it_it.yml", include_str!("../resources/lang/it_it.yml")),
    ("ja_jp.yml", include_str!("../resources/lang/ja_jp.yml")),
    ("pt_br.yml", include_str!("../resources/lang/pt_br.yml")),
    ("zh_cn.yml", include_str!("../resources/lang/zh_cn.yml")),
];
const DEFAULT_CONFIG: &str = include_str!("../resources/config.yml");
const FALLBACK_TOOL: &str = "knowledge_book";
const MAX_SLOTS: usize = 54;

struct GuiItem {
    name: String,
    command: String,
    key: String,
    run_as_player: bool,
    lore: Vec<String>,
    cooldown: i64,
    verbose: Option<bool>,
}

/// Everything read from config.yml and the language file; replaced as a whole on reload.
struct State {
    items: BTreeMap<i64, GuiItem>,
    messages: serde_yaml::Value,
    verbose: bool,
    fixed_size: bool,
    tool_key: String,
}

static STATE: RwLock<Option<Arc<State>>> = RwLock::new(None);
/// Players who have the CommandGUI window open (the click event carries no title to compare).
static OPEN: Mutex<Option<HashSet<String>>> = Mutex::new(None);
/// player name -> slot -> last use in ms.
static COOLDOWNS: Mutex<Option<HashMap<String, HashMap<i64, i64>>>> = Mutex::new(None);
fn state() -> Arc<State> {
    STATE
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .expect("loaded in on_load")
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// Legacy `&` codes become `§`, like Bukkit's getMessage / translateAlternateColorCodes.
fn amp(s: &str) -> String {
    s.replace('&', "§")
}

fn text(legacy: &str) -> TextComponent {
    TextComponent::from_legacy_string(legacy)
}

/// Material.matchMaterial for current item names: case-insensitive, optional namespace, spaces as underscores.
fn match_item(name: &str) -> Option<String> {
    let key = name.trim().to_lowercase().replace(' ', "_");
    let key = key.strip_prefix("minecraft:").unwrap_or(&key).to_string();
    Item::from_registry_key(&key).map(|_| key)
}

impl State {
    fn msg(&self, key: &str) -> String {
        let value = self
            .messages
            .get("messages")
            .and_then(|m| m.get(key))
            .and_then(|v| v.as_str());
        amp(value.unwrap_or(key))
    }

    fn title(&self) -> TextComponent {
        text(&self.msg("gui_title"))
    }
}

fn load() -> State {
    let config_path = format!("{DATA}/config.yml");
    if !Path::new(&config_path).exists() {
        let _ = std::fs::write(&config_path, DEFAULT_CONFIG);
    }
    let config: serde_yaml::Value = std::fs::read_to_string(&config_path)
        .ok()
        .and_then(|s| serde_yaml::from_str(&s).ok())
        .unwrap_or(serde_yaml::Value::Null);

    let lang_dir = format!("{DATA}/lang");
    let _ = std::fs::create_dir_all(&lang_dir);
    for (file, content) in LANG_FILES {
        let path = format!("{lang_dir}/{file}");
        if !Path::new(&path).exists() {
            match std::fs::write(&path, content) {
                Ok(()) => tracing::info!("Language file created: {file}"),
                Err(_) => tracing::warn!("Could not create language file: {file}"),
            }
        }
    }
    let wanted = config
        .get("language-file")
        .and_then(|v| v.as_str())
        .unwrap_or("en_us.yml");
    let mut lang_file = wanted.to_string();
    if !Path::new(&format!("{lang_dir}/{lang_file}")).exists() {
        tracing::warn!("Configured language file not found: {wanted}. Falling back to en_us.yml.");
        lang_file = "en_us.yml".into();
    }
    let messages = std::fs::read_to_string(format!("{lang_dir}/{lang_file}"))
        .ok()
        .and_then(|s| serde_yaml::from_str(&s).ok())
        .unwrap_or(serde_yaml::Value::Null);
    tracing::info!("Loaded language file: {lang_file}");

    let tool_key = match config.get("gui-tool").and_then(|v| v.as_str()) {
        None | Some("") => {
            tracing::warn!("[CommandGUI] Material was null/empty. Using fallback: KNOWLEDGE_BOOK");
            FALLBACK_TOOL.to_string()
        }
        Some(name) => match_item(name).unwrap_or_else(|| {
            tracing::warn!(
                "[CommandGUI] Invalid material '{name}'. Using fallback: KNOWLEDGE_BOOK"
            );
            FALLBACK_TOOL.to_string()
        }),
    };

    let mut items = BTreeMap::new();
    let list = config
        .get("gui-items")
        .and_then(|v| v.as_sequence())
        .cloned()
        .unwrap_or_default();
    for entry in list {
        let str_of = |k: &str| {
            entry
                .get(k)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string()
        };
        let name = str_of("name");
        let material = str_of("item");
        let Some(key) = match_item(&material) else {
            tracing::warn!("Invalid material '{material}' for item: {name}");
            continue;
        };
        let slot = entry
            .get("slot")
            .and_then(|v| v.as_i64())
            .unwrap_or(items.len() as i64);
        let lore = entry
            .get("lore")
            .and_then(|v| v.as_sequence())
            .map(|l| {
                l.iter()
                    .map(|x| x.as_str().map_or_else(|| format!("{x:?}"), String::from))
                    .collect()
            })
            .unwrap_or_default();
        tracing::info!("Loaded GUI item: {name} at slot {slot}");
        items.insert(
            slot,
            GuiItem {
                command: str_of("command"),
                name,
                key,
                run_as_player: entry
                    .get("run-as-player")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                lore,
                cooldown: entry.get("cooldown").and_then(|v| v.as_i64()).unwrap_or(0),
                verbose: entry.get("verbose").and_then(|v| v.as_bool()),
            },
        );
    }
    State {
        items,
        messages,
        verbose: config
            .get("verbose-mode")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        fixed_size: config
            .get("gui-size-mode")
            .and_then(|v| v.as_str())
            .is_some_and(|m| m.eq_ignore_ascii_case("fixed")),
        tool_key,
    }
}

fn reload() {
    let new = Arc::new(load());
    *STATE.write().unwrap_or_else(|e| e.into_inner()) = Some(new);
}

fn open_gui(player: &Player) {
    let st = state();
    // shortcut: Pumpkin screens are 9x1..9x6 only, so more than 54 slots are cut off (Bukkit would throw).
    let size = if st.fixed_size {
        MAX_SLOTS
    } else {
        ((st.items.len() / 9) + 1) * 9
    }
    .min(MAX_SLOTS);
    let screen = [
        Screen::Generic9x1,
        Screen::Generic9x2,
        Screen::Generic9x3,
        Screen::Generic9x4,
        Screen::Generic9x5,
        Screen::Generic9x6,
    ][size / 9 - 1];
    let gui = Gui::new(screen, st.title());
    gui.set_allow_grab_items(false);
    gui.set_allow_put_items(false);
    for (slot, item) in &st.items {
        if *slot < 0 || *slot as usize >= size {
            tracing::warn!(
                "GUI item '{}' has slot {slot} outside the {size}-slot window",
                item.name
            );
            continue;
        }
        let stack = ItemStack::new(&item.key, 1);
        stack.set_custom_name(Some(text(&amp(&item.name))));
        stack.set_lore(item.lore.iter().map(|l| text(&amp(l))).collect());
        gui.set_item(*slot as u32, stack);
    }
    OPEN.lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_default()
        .insert(player.get_name());
    player.open_gui(gui);
}

fn give_tool(player: &Player) {
    let st = state();
    let stack = ItemStack::new(&st.tool_key, 1);
    stack.set_custom_name(Some(text(&st.msg("tool_name"))));
    stack.set_lore(vec![text(&st.msg("tool_description"))]);
    // Bukkit's addItem fills the first free storage slot; a full inventory just loses the item (no drop).
    let inv = player.get_inventory().as_inventory();
    if let Some(slot) = (0..inv.get_size().min(36)).find(|s| inv.get_item(*s).is_none()) {
        inv.set_item(slot, Some(stack));
    }
}

fn red(st: &State, key: &str) -> TextComponent {
    text(&format!("§c{}", st.msg(key)))
}

struct Cg;

impl CommandHandler for Cg {
    fn handle(
        &self,
        sender: CommandSender,
        server: Server,
        args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let raw = match args.get_value("args") {
            pumpkin_plugin_api::command::Arg::Simple(s)
            | pumpkin_plugin_api::command::Arg::Msg(s) => s,
            _ => String::new(),
        };
        run(
            &sender,
            &server,
            raw.split(' ').filter(|s| !s.is_empty()).collect(),
        );
        Ok(1)
    }
}

fn run(sender: &CommandSender, server: &Server, args: Vec<&str>) {
    let st = state();
    let say = |t: TextComponent| sender.send_message(t);
    let allowed = |node: &str| sender.has_permission(server, node);
    let Some(sub) = args.first().map(|s| s.to_lowercase()) else {
        match sender.as_player() {
            Some(p) if allowed("commandgui.use") => open_gui(&p),
            Some(_) => say(red(&st, "no_permission")),
            None => say(red(&st, "only_players")),
        }
        return;
    };
    match sub.as_str() {
        "help" => {
            let line = |c: &str, d: &str| say(text(&format!("§e{c}§f - {d}")));
            say(text("§6CommandGUI Help:"));
            line("/commandgui", "Opens the Command GUI.");
            line("/commandgui tool", "Gives the player a CommandGUI Tool.");
            line(
                "/commandgui give <player>",
                "Gives the specified player a CommandGUI Tool.",
            );
            line("/commandgui reload", "Reloads the plugin configuration.");
            line("/commandgui help", "Displays this help message.");
            line("/cg", "/cg is an alias for /commandgui");
        }
        "reload" if allowed("commandgui.reload") => {
            reload();
            say(text(&format!("§a{}", state().msg("reload_success"))));
        }
        "tool" => match sender.as_player() {
            Some(p) if allowed("commandgui.tool") => {
                give_tool(&p);
                say(text(&format!("§a{}", st.msg("tool_received"))));
            }
            Some(_) => say(red(&st, "no_permission")),
            None => say(red(&st, "only_players")),
        },
        "give" if allowed("commandgui.give") => match args.get(1) {
            None => say(red(&st, "invalid_usage")),
            Some(name) => match server.get_player_by_name(name) {
                Some(target) => {
                    give_tool(&target);
                    say(text(&format!(
                        "§a{}",
                        st.msg("tool_given").replace("%player%", name)
                    )));
                    target.send_system_message(
                        text(&format!("§a{}", st.msg("tool_received"))),
                        false,
                    );
                }
                None => say(text(&format!(
                    "§c{}",
                    st.msg("player_not_found").replace("%player%", name)
                ))),
            },
        },
        "reload" | "give" => say(red(&st, "no_permission")),
        _ => say(red(&st, "invalid_subcommand")),
    }
}

struct CgSuggest;

impl CommandSuggestionHandler for CgSuggest {
    fn suggest(
        &self,
        sender: CommandSender,
        server: Server,
        request: SuggestionRequest,
    ) -> CommandSuggestions {
        // `remaining` is the part of the greedy argument that gets replaced; complete its last word.
        let rem = request.remaining.as_str();
        let word_at = rem.rfind(' ').map_or(0, |i| i + 1);
        let (done, last) = rem.split_at(word_at);
        let prior: Vec<&str> = done.split(' ').filter(|s| !s.is_empty()).collect();
        let values: Vec<String> = match prior.as_slice() {
            [] => ["reload", "tool", "give", "help"]
                .into_iter()
                .filter(|s| {
                    matches!(*s, "tool" | "help")
                        || sender.has_permission(&server, &format!("commandgui.{s}"))
                })
                .filter(|s| s.starts_with(&last.to_lowercase()))
                .map(String::from)
                .collect(),
            [first] if first.eq_ignore_ascii_case("give") => server
                .get_all_players()
                .iter()
                .map(Player::get_name)
                .filter(|n| n.starts_with(&last.to_lowercase()))
                .collect(),
            _ => vec![],
        };
        CommandSuggestions {
            start: request.start + word_at as u32,
            length: last.len() as u32,
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

struct Click;

impl EventHandler<InventoryClickEvent> for Click {
    fn handle(
        &self,
        server: Server,
        mut event: InventoryClickEventData,
    ) -> InventoryClickEventData {
        let name = event.player.get_name();
        let is_ours = OPEN
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .is_some_and(|s| s.contains(&name));
        if !is_ours {
            return event;
        }
        event.cancelled = true;
        let st = state();
        // Only clicks in the menu itself count (the original also matched a same-numbered slot of the player's inventory).
        let Some(item) = usize::try_from(event.raw_slot)
            .ok()
            .filter(|s| *s < MAX_SLOTS)
            .and_then(|_| st.items.get(&i64::from(event.raw_slot)))
            .filter(|_| event.clicked_item.is_some())
        else {
            return event;
        };
        let slot = i64::from(event.raw_slot);
        let player = &event.player;
        if !player.has_permission("commandgui.bypass") {
            let now = now_ms();
            let mut all = COOLDOWNS.lock().unwrap_or_else(|e| e.into_inner());
            let mine = all.get_or_insert_default().entry(name.clone()).or_default();
            let last = mine.get(&slot).copied().unwrap_or(0);
            if now - last < item.cooldown * 1000 {
                let left = (item.cooldown * 1000 - (now - last)) / 1000;
                drop(all);
                let msg = st
                    .msg("cooldown_active")
                    .replace("%time%", &left.to_string());
                player.send_system_message(text(&format!("§c{msg}")), false);
                return event;
            }
            mine.insert(slot, now);
        }
        if item.verbose == Some(true) || (item.verbose.is_none() && st.verbose) {
            tracing::info!(
                "{}",
                st.msg("verbose_message")
                    .replace("%player%", &name)
                    .replace("%name%", &item.name)
            );
        }
        let command = item.command.replace("%player%", &name);
        let sender = match server
            .get_player_by_name(&name)
            .filter(|_| item.run_as_player)
        {
            Some(p) => ServerSender::Player(p),
            None => ServerSender::Console,
        };
        server.execute_command(&command, sender);
        event
    }
}

struct Close;

impl EventHandler<InventoryCloseEvent> for Close {
    fn handle(&self, _server: Server, event: InventoryCloseEventData) -> InventoryCloseEventData {
        if let Some(set) = OPEN.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            set.remove(&event.player.get_name());
        }
        event
    }
}

struct Interact;

impl EventHandler<PlayerInteractEvent> for Interact {
    fn handle(
        &self,
        _server: Server,
        mut event: PlayerInteractEventData,
    ) -> PlayerInteractEventData {
        let Some(held) = event.player.get_inventory().get_item_in_hand(Hand::Right) else {
            return event;
        };
        let st = state();
        if held.get_registry_key().trim_start_matches("minecraft:") != st.tool_key {
            return event;
        }
        // Same custom name as the tool /cg tool hands out.
        let is_tool = held
            .get_custom_name()
            .is_some_and(|n| n.to_json() == text(&st.msg("tool_name")).to_json());
        if is_tool {
            event.cancelled = true;
            open_gui(&event.player);
        }
        event
    }
}

struct CommandGui;

impl Plugin for CommandGui {
    fn new() -> Self {
        CommandGui
    }

    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "CommandGUI".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            authors: vec!["Alfie51m".into()],
            description: "Main command for the CommandGUI plugin.".into(),
            dependencies: vec![],
            permissions: vec![permissions::FS_WRITE_DATA.into()],
        }
    }

    fn on_load(&self, context: Context) -> Result<(), String> {
        reload();
        for (node, description) in [
            (
                "commandgui.use",
                "Allows access to the /commandgui command and GUI.",
            ),
            (
                "commandgui.tool",
                "Allows access to /commandgui tool to get a CommandGUI Tool.",
            ),
            (
                "commandgui.give",
                "Allows access to /commandgui give <player> to give the CommandGUI Tool to another player.",
            ),
            (
                "commandgui.reload",
                "Allows access to /commandgui reload to reload the configuration.",
            ),
            ("commandgui.bypass", "Skips the cooldowns of GUI items."),
        ] {
            context.register_permission(&Permission {
                node: node.into(),
                description: description.into(),
                default: PermissionDefault::Op(PermissionLevel::One),
                children: vec![],
            })?;
        }
        // The command itself is open to everybody (subcommands check their own nodes), like in the original.
        context.register_permission(&Permission {
            node: "commandgui:command".into(),
            description:
                "Lets everybody run /commandgui; the subcommands check their own permissions."
                    .into(),
            default: PermissionDefault::Allow,
            children: vec![],
        })?;
        let args = CommandNode::argument("args", &ArgumentType::String(StringType::Greedy))
            .suggest(CgSuggest)
            .execute(Cg);
        let command = Command::new(
            &["commandgui".into(), "cg".into()],
            "Main command for the CommandGUI plugin.",
        )
        .execute(Cg)
        .then(args);
        context.register_command(command, "commandgui:command");
        context.register_event_handler::<InventoryClickEvent, _>(
            Click,
            EventPriority::Normal,
            true,
        )?;
        context.register_event_handler::<InventoryCloseEvent, _>(
            Close,
            EventPriority::Normal,
            false,
        )?;
        context.register_event_handler::<PlayerInteractEvent, _>(
            Interact,
            EventPriority::Normal,
            true,
        )?;
        tracing::info!("CommandGUI Plugin has been enabled.");
        Ok(())
    }

    fn on_unload(&self, _context: Context) -> Result<(), String> {
        tracing::info!("CommandGUI Plugin has been disabled.");
        Ok(())
    }
}

register_plugin!(CommandGui);
