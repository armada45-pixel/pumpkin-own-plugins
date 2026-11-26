//! AfkPool: rewards players who stand in a WorldGuard region. Rust port of AfkPool 2.2.0 by kmaba (MIT);
//! see README.md for the differences.

mod regions;

use pumpkin_plugin_api::{
    Context, EventHandler, Player, Plugin, PluginMetadata, Result, Server,
    command::{
        Arg, ArgumentType, Command, CommandError, CommandNode, CommandSender, CommandSuggestion,
        CommandSuggestions, ConsumedArgs, StringType, SuggestionRequest,
    },
    commands::{CommandHandler, CommandSuggestionHandler},
    events::{EventPriority, PlayerLeaveEvent, PlayerMoveEvent},
    events_wit::{PlayerLeaveEventData, PlayerMoveEventData},
    permission::{Permission, PermissionDefault, PermissionLevel},
    permissions::{FS_READ_DATA, FS_WRITE_DATA},
    register_plugin,
    scheduler::{cancel_task, schedule_delayed_task, schedule_repeating_task},
    server::CommandSender as ServerSender,
    text::TextComponent,
};
use serde_yaml_ng::Value;
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use tracing::{error, info};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_CONFIG: &str = include_str!("../resources/config.yml");
const DEFAULT_TIMER_FORMAT: &str = "&7Next reward in &e%time%";
const LINE: &str = "--------------------------------";

struct CommandConfig {
    key: String,
    region_name: Option<String>,
    interval: i64,
    command: Option<String>,
    title: Option<String>,
    enabled: bool,
    min: i32,
    max: i32,
    multiplier: f64,
}

#[derive(Default)]
struct State {
    data_dir: String,
    commands: Vec<Arc<CommandConfig>>,
    subtitle: Option<String>,
    entering_title: Option<String>,
    exiting_title: Option<String>,
    timer_enabled: bool,
    timer_format: String,
    /// Player names inside each command's region, by command key.
    players_in_regions: HashMap<String, HashSet<String>>,
    next_reward_ms: HashMap<String, i64>,
    tasks: Vec<u32>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    f(STATE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(State::default))
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// `Math.random()` stand-in: xorshift seeded from the clock.
fn random() -> f64 {
    static SEED: Mutex<u64> = Mutex::new(0);
    let mut s = SEED.lock().unwrap_or_else(|e| e.into_inner());
    if *s == 0 {
        *s = (now_ms() as u64) | 1;
    }
    *s ^= *s << 13;
    *s ^= *s >> 7;
    *s ^= *s << 17;
    (*s >> 11) as f64 / (1u64 << 53) as f64
}

// ---- Bukkit-style config reads (a missing or wrongly typed key gives the type's zero, like YamlConfiguration) ----

fn path<'a>(cfg: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().try_fold(cfg, |v, k| v.get(*k))
}
fn get_string(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}
fn get_f64(v: Option<&Value>) -> Option<f64> {
    match v? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}
fn get_bool(v: Option<&Value>) -> Option<bool> {
    match v? {
        Value::Bool(b) => Some(*b),
        Value::String(s) if s.eq_ignore_ascii_case("true") => Some(true),
        Value::String(s) if s.eq_ignore_ascii_case("false") => Some(false),
        _ => None,
    }
}

fn save_default_config(dir: &str) {
    let file = format!("{dir}/config.yml");
    if !std::path::Path::new(&file).exists() {
        let _ = std::fs::create_dir_all(dir);
        if let Err(e) = std::fs::write(&file, DEFAULT_CONFIG) {
            error!("Could not save the default config.yml: {e}");
        }
    }
}

/// `reload()` of the original: re-reads config.yml and clears the runtime state.
fn load_config() {
    with_state(|s| {
        s.commands.clear();
        s.players_in_regions.clear();
        s.next_reward_ms.clear();
        let text =
            std::fs::read_to_string(format!("{}/config.yml", s.data_dir)).unwrap_or_default();
        let cfg: Value = serde_yaml_ng::from_str(&text).unwrap_or(Value::Null);
        s.subtitle = get_string(cfg.get("subtitle"));
        s.entering_title = get_string(cfg.get("entering-title"));
        s.exiting_title = get_string(cfg.get("exiting-title"));
        s.timer_enabled = get_bool(path(&cfg, &["timer", "enabled"])).unwrap_or(true);
        s.timer_format = get_string(path(&cfg, &["timer", "format"]))
            .filter(|f| !f.is_empty())
            .unwrap_or_else(|| DEFAULT_TIMER_FORMAT.to_string());
        match cfg.get("commands").and_then(Value::as_mapping) {
            Some(map) => {
                for (key, c) in map {
                    let Some(key) = get_string(Some(key)) else {
                        continue;
                    };
                    s.players_in_regions.insert(key.clone(), HashSet::new());
                    s.commands.push(Arc::new(CommandConfig {
                        region_name: get_string(c.get("region-name")),
                        interval: get_f64(c.get("interval")).unwrap_or(0.0) as i64,
                        command: get_string(c.get("command")),
                        title: get_string(c.get("title")),
                        enabled: get_bool(c.get("enabled")).unwrap_or(false),
                        min: get_f64(c.get("min")).unwrap_or(0.0) as i32,
                        max: get_f64(c.get("max")).unwrap_or(0.0) as i32,
                        multiplier: get_f64(c.get("multiplier")).unwrap_or(1.0),
                        key,
                    }));
                }
            }
            None => error!("No commands found in the config.yml file."),
        }
    });
}

/// `onEnable()` of the original, minus the update checker and bStats.
fn enable() {
    info!("AfkPool Version {VERSION} enabled.");
    let dir = with_state(|s| s.data_dir.clone());
    save_default_config(&dir);
    load_config();
    let (commands, timer) = with_state(|s| (s.commands.clone(), s.timer_enabled));
    let mut ids = Vec::new();
    for c in commands.into_iter().filter(|c| c.enabled) {
        with_state(|s| {
            s.next_reward_ms
                .insert(c.key.clone(), now_ms() + c.interval * 50)
        });
        // Bukkit rejects a period below 1 tick; so does the host, so clamp.
        ids.push(schedule_repeating_task(
            1,
            c.interval.max(1) as u64,
            move |server| execute_command_for_region(&server, &c),
        ));
    }
    if timer {
        ids.push(schedule_repeating_task(20, 20, |server| {
            update_reward_timers(&server)
        }));
    }
    with_state(|s| s.tasks = ids);
}

/// `disablePlugin()` of the original: stops every timer.
fn disable() {
    for id in with_state(|s| std::mem::take(&mut s.tasks)) {
        cancel_task(id);
    }
    info!("AfkPool Disabled");
}

fn format(text: &str) -> TextComponent {
    TextComponent::from_legacy_string_with_code(text, '&')
}

fn send_title(player: &Player, title: &str, subtitle: Option<&str>) {
    player.send_title_animation(10, 70, 20);
    player.show_title(format(title));
    if let Some(sub) = subtitle {
        player.show_subtitle(format(sub));
    }
}

fn in_region(player: &Player, region: Option<&str>) -> bool {
    let Some(region) = region else { return false };
    let dir = with_state(|s| s.data_dir.clone());
    regions::in_region(
        &dir,
        &player.get_world().get_name(),
        player.get_position(),
        region,
    )
}

fn execute_command_for_region(server: &Server, c: &CommandConfig) {
    with_state(|s| {
        s.next_reward_ms
            .insert(c.key.clone(), now_ms() + c.interval * 50)
    });
    for player in server.get_all_players() {
        let name = player.get_name();
        if in_region(&player, c.region_name.as_deref()) {
            let first = with_state(|s| {
                s.players_in_regions
                    .entry(c.key.clone())
                    .or_default()
                    .insert(name)
            });
            if first {
                let sub = with_state(|s| s.subtitle.clone());
                send_title(&player, c.title.as_deref().unwrap_or(""), sub.as_deref());
            }
            execute_command_for_player(server, &player, c);
        } else {
            with_state(|s| {
                s.players_in_regions
                    .entry(c.key.clone())
                    .or_default()
                    .remove(&name)
            });
        }
    }
}

fn update_reward_timers(server: &Server) {
    let now = now_ms();
    let (commands, next, format_str) = with_state(|s| {
        (
            s.commands.clone(),
            s.next_reward_ms.clone(),
            s.timer_format.clone(),
        )
    });
    for player in server.get_all_players() {
        let soonest = commands
            .iter()
            .filter(|c| c.enabled)
            .filter_map(|c| {
                next.get(&c.key)
                    .copied()
                    .filter(|n| *n > now)
                    .map(|n| (c, n))
            })
            .filter(|(c, _)| in_region(&player, c.region_name.as_deref()))
            .map(|(_, n)| n)
            .min();
        if let Some(at) = soonest {
            player.show_actionbar(format(
                &format_str.replace("%time%", &format_duration(at - now)),
            ));
        }
    }
}

/// `Countdown.formatDuration`: m:ss, or h:mm:ss from one hour, rounded up to whole seconds.
fn format_duration(ms: i64) -> String {
    let total = ((ms + 999) / 1000).max(0);
    let (h, m, s) = (total / 3600, total % 3600 / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

fn execute_command_for_player(server: &Server, player: &Player, c: &CommandConfig) {
    if !c.enabled {
        return;
    }
    let mut value = c.min + (random() * f64::from(c.max - c.min + 1)) as i32;
    if player.has_permission("afkpool.bonus") {
        value *= c.multiplier as i32;
    }
    let cmd = c
        .command
        .clone()
        .unwrap_or_default()
        .replace("%p", &player.get_name())
        .replace("%m", &value.to_string());
    // Next tick, from the console, like the original's runNextTick(dispatchCommand).
    schedule_delayed_task(1, move |server| {
        server.execute_command(&cmd, ServerSender::Console)
    });
    let _ = server;
    let sub = with_state(|s| s.subtitle.clone());
    send_title(
        player,
        &c.title
            .clone()
            .unwrap_or_default()
            .replace("%m", &value.to_string()),
        sub.as_deref(),
    );
}

// ---- events ----

struct MoveHandler;
impl EventHandler<PlayerMoveEvent> for MoveHandler {
    fn handle(&self, _server: Server, event: PlayerMoveEventData) -> PlayerMoveEventData {
        let player = &event.player;
        let name = player.get_name();
        let commands = with_state(|s| s.commands.clone());
        for c in commands {
            let inside = in_region(player, c.region_name.as_deref());
            let (changed, title) = with_state(|s| {
                let set = s.players_in_regions.entry(c.key.clone()).or_default();
                if inside {
                    (set.insert(name.clone()), s.entering_title.clone())
                } else {
                    (set.remove(&name), s.exiting_title.clone())
                }
            });
            if changed {
                send_title(player, title.as_deref().unwrap_or(""), None);
            }
        }
        event
    }
}

struct LeaveHandler;
impl EventHandler<PlayerLeaveEvent> for LeaveHandler {
    fn handle(&self, _server: Server, event: PlayerLeaveEventData) -> PlayerLeaveEventData {
        let name = event.player.get_name();
        with_state(|s| {
            s.players_in_regions.values_mut().for_each(|set| {
                set.remove(&name);
            })
        });
        event
    }
}

// ---- /AfkPool ----

fn legacy(s: &str) -> TextComponent {
    TextComponent::from_legacy_string(s)
}

const USAGE: &str = "/AfkPool <help|reload|test|values>";

struct Cmd {
    with_args: bool,
}

impl CommandHandler for Cmd {
    fn handle(
        &self,
        sender: CommandSender,
        server: Server,
        args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let words: Vec<String> = if self.with_args {
            match args.get_value("args") {
                Arg::Simple(s) | Arg::Msg(s) => s.split_whitespace().map(str::to_string).collect(),
                _ => vec![],
            }
        } else {
            vec![]
        };
        let say = |s: &str| sender.send_message(legacy(s));
        let sub = if let [one] = words.as_slice() {
            one.to_lowercase()
        } else {
            String::new()
        };
        match sub.as_str() {
            "help" => {
                say(LINE);
                say("§a/AfkPool reload - Reloads config, also restarts the plugin.");
                say("§a/AfkPool test - Give tester things set in the config");
                say("§a/AfkPool values - Shows values of the config");
                say("§a/AfkPool version - Shows the version number");
                say(
                    "§6§lTIP: §r§9Use '/AfkPool' shortcut '/ap' for easier access to the plugins commands",
                );
                say(LINE);
            }
            "version" => {
                // shortcut: the Spigot update check is not ported, so the "version on Spigot" is unknown ("null",
                // as the original prints when its check fails).
                say(LINE);
                say(&format!("§6The version you have installed is §e{VERSION}"));
                say("§6The version on Spigot is §enull");
                say("§cYou are not on the latest version!");
                say(LINE);
            }
            "reload" => {
                disable();
                enable();
                say(LINE);
                say("§2Config reloaded!");
                say(LINE);
            }
            "test" => {
                // Original: Bukkit.getPlayer(sender name), a console sender fails with an exception. Here: nothing.
                if let Some(player) = sender.as_player() {
                    let commands = with_state(|s| s.commands.clone());
                    // The original sleeps 500 ms (10 ticks) between commands on the main thread; schedule instead.
                    for (i, c) in commands.into_iter().filter(|c| c.enabled).enumerate() {
                        let player = player.get_name();
                        schedule_delayed_task(1 + 10 * i as u64, move |server| {
                            if let Some(p) = server.get_player_by_name(&player) {
                                execute_command_for_player(&server, &p, &c);
                            }
                        });
                    }
                    let _ = server;
                }
            }
            "values" => {
                say(LINE);
                for c in with_state(|s| s.commands.clone()) {
                    let opt = |o: &Option<String>| o.clone().unwrap_or_else(|| "null".into());
                    say(&format!("§9Command: §6{}", c.key));
                    say(&format!("§9region-name: §6{}", opt(&c.region_name)));
                    say(&format!("§9command: §6{}", opt(&c.command)));
                    say(&format!("§9title: §6{}", opt(&c.title)));
                    say(&format!("§9enabled: {}", c.enabled));
                    say(&format!("§9interval: §a{}", c.interval));
                    say(&format!("§9min: §f{}", c.min));
                    say(&format!("§9max: §f{}", c.max));
                    say(&format!("§9multiplier: §f{}", java_double(c.multiplier)));
                    say(LINE);
                }
            }
            // Bukkit prints the usage from plugin.yml when onCommand returns false.
            _ => say(USAGE),
        }
        Ok(1)
    }
}

/// `String.valueOf(double)`: always has a decimal point.
fn java_double(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e7 {
        format!("{v:.1}")
    } else {
        v.to_string()
    }
}

/// Bukkit has no tab completer for this command, so it completes online player names.
struct PlayerNames;
impl CommandSuggestionHandler for PlayerNames {
    fn suggest(
        &self,
        _sender: CommandSender,
        server: Server,
        request: SuggestionRequest,
    ) -> CommandSuggestions {
        let start = request.input.rfind(' ').map_or(0, |i| i + 1);
        let prefix = request.input[start..].to_lowercase();
        let values = server
            .get_all_players()
            .into_iter()
            .map(|p| p.get_name())
            .filter(|n| n.to_lowercase().starts_with(&prefix))
            .map(|value| CommandSuggestion {
                value,
                tooltip: None,
            })
            .collect();
        CommandSuggestions {
            start: start as u32,
            length: (request.input.len() - start) as u32,
            values,
        }
    }
}

struct AfkPool;

impl Plugin for AfkPool {
    fn new() -> Self {
        AfkPool
    }

    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "AfkPool".into(),
            version: VERSION.into(),
            authors: vec!["kmaba".into()],
            // Same text as the original plugin.yml.
            description: "A plugin that rewards AFK players in a specific region".into(),
            // The original depends on WorldGuard, a Java plugin the Wasm host cannot see (see HOST-API.md).
            dependencies: vec![],
            permissions: vec![FS_READ_DATA.into(), FS_WRITE_DATA.into()],
        }
    }

    fn on_load(&self, context: Context) -> Result<()> {
        with_state(|s| s.data_dir = context.get_data_folder());
        context.register_permission(&Permission {
            node: "afkpool.bonus".into(),
            description: "Allows the player to receive multiplied rewards".into(),
            default: PermissionDefault::Deny,
            children: vec![],
        })?;
        // afkpool.use is not declared in the original plugin.yml: Bukkit treats it as op-only.
        context.register_permission(&Permission {
            node: "afkpool.use".into(),
            description: String::new(),
            default: PermissionDefault::Op(PermissionLevel::One),
            children: vec![],
        })?;
        context.register_event_handler::<PlayerMoveEvent, _>(
            MoveHandler,
            EventPriority::Normal,
            false,
        )?;
        context.register_event_handler::<PlayerLeaveEvent, _>(
            LeaveHandler,
            EventPriority::Normal,
            false,
        )?;
        // Bukkit matches command names case-insensitively; the lowercase name stands in for that.
        let command = Command::new(
            &["AfkPool".into(), "afkpool".into(), "ap".into()],
            "Triggers events with in AfkPool and shows the values, permission is afkpool.use",
        );
        let command = command
            .then(
                CommandNode::argument("args", &ArgumentType::String(StringType::Greedy))
                    .suggest(PlayerNames)
                    .execute(Cmd { with_args: true }),
            )
            .execute(Cmd { with_args: false });
        context.register_command(command, "afkpool.use");
        enable();
        Ok(())
    }

    fn on_unload(&self, _context: Context) -> Result<()> {
        disable();
        Ok(())
    }
}

register_plugin!(AfkPool);
