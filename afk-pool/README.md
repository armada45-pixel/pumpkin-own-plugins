# AfkPool (Rust / Pumpkin)

Rust port of [AfkPool](https://github.com/kmaba/AfkPool) 2.2.0 by kmaba ("AFK Pool/Lake/Room/Pit Rewards"),
built as a Pumpkin Wasm plugin. MIT licensed like the original (see `LICENSE`).
Status: written and built, not yet tested on a server. Region lookup needs host support, see `HOST-API.md`.

## What players and admins see (checklist)

- Plugin name `AfkPool`, version `2.2.0`, author kmaba, description "A plugin that rewards AFK players in a specific region".
- Command `/AfkPool` (alias `/ap`), permission `afkpool.use` (not declared in the original plugin.yml: op-only),
  description and usage `/AfkPool <help|reload|test|values>`.
  - no arguments, or anything else: prints the usage line.
  - `help`: the `----` bordered help text with the `/ap` tip.
  - `version`: installed version; the Spigot version line is a stand-in (see differences).
  - `reload`: re-reads `config.yml`, restarts all timers, prints `Config reloaded!`.
  - `test`: gives the caller every enabled reward, 0.5 s apart.
  - `values`: every command block of the config with its colours.
- Permission `afkpool.bonus` (default false): rewards are multiplied by `(int) multiplier`.
- `config.yml`: same file name, keys, defaults and comments (`subtitle`, `entering-title`, `exiting-title`,
  `timer.enabled/format`, `commands.<key>.region-name/interval/command/title/enabled/min/max/multiplier`).
  `%p` and `%m` in `command`, `%m` in `title`, `%time%` in `timer.format`; `&` colour codes.
- Rewards: every `interval` ticks (first run after 1 tick) each online player standing in the command's region gets
  `command` run from the console one tick later, plus a title (fade 10/70/20) and the subtitle.
- Entering or leaving a region on the next move shows `entering-title` / `exiting-title`.
- Action bar `timer.format` every second with the time to the soonest reward of the regions the player is in.

## Differences inside

- **Regions (blocker)**: the original asks WorldGuard; the port reads a copy of WorldGuard's `regions.yml` from its
  own data folder (`<data>/WorldGuard/worlds/<world>/regions.yml`), 10 s cache. Poly2d uses a plain ray-cast.
  See `HOST-API.md`. The plugin does not declare WorldGuard as a dependency for the same reason.
- **Data folder**: a Wasm plugin's folder is `plugins/data/AfkPool/`, not `plugins/AfkPool/`; copy the live
  `config.yml` there (the port writes the shipped default if the file is missing).
- **Update checker and bStats removed**: no network access is requested. `/afkpool version` prints
  `The version on Spigot is null` and "not on the latest version", which is what the original prints when its
  check fails.
- **`/afkpool test`**: the original sleeps 500 ms per reward on the main thread; the port schedules each reward
  10 ticks apart. From the console the original throws; the port does nothing.
- **`reload`**: instead of disabling and re-enabling the plugin it cancels its timers and starts them again.
- **Commands are matched case-sensitively** by the host, so `afkpool` is registered next to `AfkPool` and `ap`.
- **Tab-complete**: the original has none (Bukkit completes online player names); the port suggests the same names.
- Config is parsed with serde_yaml_ng and read with Bukkit's rules (missing number: 0, `multiplier` default 1.0).
  Command blocks keep config order (the original uses a hash map).
- Region membership is keyed by player name instead of the player object.

## Server requirements

- Pumpkin with the Wasm plugin host; allow the unsigned build in `pumpkin.toml`:

  ```toml
  [plugins.overrides.AfkPool]
  allow_unsigned = true
  ```
- Something that provides regions (see above).

## Build

```
cargo build --release -p afk-pool    # target wasm32-wasip2 is set in .cargo/config.toml
```
