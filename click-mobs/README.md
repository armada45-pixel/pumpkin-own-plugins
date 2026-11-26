# ClickMobs (Rust / Pumpkin)

Sneak + right-click a mob to pick it up as a player head, place the head to put the mob back. Rust port of
[ClickMobs](https://github.com/Clickism/ClickMobs) 1.3.1 by Clickism (Paper plugin, `paper/` of the repo at `d4ff8bc`),
built as a Pumpkin Wasm plugin. Licensed GPL-3.0-only like the original (see `LICENSE`).

**Status: written and builds clean, not yet run on a server.** The host lacks entity NBT, so a picked-up mob keeps only
its type, name and age (see `HOST-API.md`, item 1). Dispensers are not handled (item 3).

## What players and admins see (checklist)

Done = in the code and unit-tested or compiled; blocked = needs `HOST-API.md`.

- [x] Plugin name `ClickMobs`, version `1.3.1`, author Clickism, description "A simple plugin that allows you to pick up any mob."
- [x] Sneak + right-click a mob (main hand): picks it up. Skipped for players, in spectator mode, with a blacklisted
      item in hand (`blacklisted_items_in_hand`, `?all`, any `*_harness`), or a mob that is not allowed
      (actionbar `blacklisted_mob`, fail sound). No `clickmobs.pickup` permission: chat `no_permission`.
- [x] Whitelist beats blacklist; `per_mob_permissions` needs `clickmobs.pickup.<mob>` (checked for the clicked mob, not
      for the whitelist).
- [x] Item: player head with the mob's texture (same texture table), name `§e<Mob>` (`"<custom name>"` if named,
      `baby_mob` text for babies), lore from `mob.lore`, `custom_model_data` per mob.
- [x] Pick up actionbar `pick_up` (`&8< &2↑ &a%s &8>`, silent), sweep particle and `entity.player.attack.sweep` sound.
- [x] Place the head (main hand, else off hand): block placement is cancelled, the mob spawns on the block's centre facing
      the player, one head is used, `entity.player.attack.weak` sound. No `clickmobs.place`: `no_permission`.
- [x] Sneak + right-click a minecart (one rider) / boat (two) / chest boat (one) with a head in the main hand: the mob
      is put in the vehicle, metal/wood break sound.
- [x] `/clickmobs config` (permission `clickmobs.config`, default op): `path`, `reload`, `get <option>` (tab-complete of the
      option names), `set <language|check_update|per_mob_permissions|enable_dispensers> <value>`. Chat messages
      `config_path`, `config_reload`, `config_get`, `config_set` with the `&6[⚒] &a` prefix and the confirm chime.
- [x] Permissions `clickmobs.pickup` (true), `clickmobs.place` (true), `clickmobs.config` (op) and the 168
      `clickmobs.pickup.<mob>` / `clickmobs.place.<mob>` nodes (false), same as plugin.yml.
- [x] `config.yml` in `plugins/ClickMobs/`: same keys, defaults, comments and layout; written when missing or when a
      key is missing. Reads the original's file as is. Tags `?all ?hostile ?baby ?tamed ?nametagged ?silent ?mob
      ?leashed ?customdata`, `not`, combined tags, same parser grammar.
- [x] `lang/<language>.json` in `plugins/ClickMobs/lang/` (en_US and de_DE shipped, copied there on first use, user
      edits win), fallback en_US, `{mob}`/`{option}`/`{value}`/`{path}` placeholders, `&` colours.
- [ ] Dispensers dispensing a mob head (blocked, item 3).
- [ ] Whole-mob snapshot on the head (blocked, item 1); riders (refused), leashed tag (item 5), update check (item 9).

## Differences inside

- **Snapshot**: the original stores the full entity SNBT in the head's persistent data (`clickmobs:entity`,
  `clickmobs:type`, `clickmobs:nbt`). The port stores the same three keys with `{id,CustomName,Age}` only, in
  `custom_data.clickmobs.*`, not Bukkit's `PublicBukkitValues` (items are not interchangeable with the jar's, item 6).
- **Riders and full inventory** refuse the pick up with `write_error` (the original stores riders, drops leftovers).
- **No update check, no bStats** (no HTTP in the host; bStats is telemetry). `check_update` stays in the file.
- **No `/reload` of the language on `language` change by editing the file**: use `/clickmobs config reload`. `/clickmobs config set`
  reloads the language itself, like the original's `onChange`.
- **Config writes** (`set`) regenerate the whole file from the template, so hand-written comments are lost; values and
  the original's comments are kept. The original's library edits in place.
- **Hostile list** is a fixed list matching Bukkit's `Monster` interface (the host has no category query).
- **Entities are found by id** near the player (the event carries only the id); no hand is reported (item 4).
- **Commands** are Pumpkin Brigadier nodes: unknown `get` options fail with "Expected literal <name>".
- Lists and the custom-model-data map cannot be set by command, as in the original.

## Server requirements

- Pumpkin with the v0.1 Wasm API of `dev/Pumpkin` (`local/teleport-stale-ack`), file permissions `fs-read-data` and
  `fs-write-data` for the data folder, and the `play-sound` fix (Pumpkin#4160).
- Unsigned build: add to `pumpkin.toml`

  ```toml
  [plugins.overrides.ClickMobs]
  allow_unsigned = true
  ```

## Build

```
dev/tools/build-own.sh -p click-mobs      # wasm32-wasip2, against dev/Pumpkin's plugin API
cargo test -p click-mobs --target x86_64-unknown-linux-gnu   # unit tests (config, lang, parser, snapshot)
```

## Tests

`dev/plugin-tests/tests/50-click-mobs.js` (config commands, pick up and place a cow); same file for jar and wasm.
