# CommandGUI (Rust / Pumpkin)

A configurable inventory menu whose items run commands (as the player or as the console), opened with `/commandgui`
(alias `/cg`) or a right-click with the CommandGUI Tool. Rust port of [CommandGUI](https://github.com/Alfie51m/CommandGUI)
3.3.0 by Alfie51m (tag `v3.3.0`), built as a Pumpkin Wasm plugin. Licensed Apache-2.0 like the original (see `LICENSE`).

## What players and admins see (checklist)

- [x] Plugin name `CommandGUI`, version `3.3.0`, author Alfie51m.
- [x] Command `/commandgui`, alias `/cg`. Open to everybody; each subcommand checks its own node:
  - `/cg` opens the menu (`commandgui.use`); the console gets `only_players`.
  - `/cg help` (no permission needed): the six help lines in the original colours.
  - `/cg tool` (`commandgui.tool`, players only): gives the tool, message `tool_received`.
  - `/cg give <player>` (`commandgui.give`): gives the tool; `invalid_usage` without a name, `player_not_found` for an
    unknown name (exact name match, online players only), `tool_given` to the sender and `tool_received` to the target.
  - `/cg reload` (`commandgui.reload`): reloads config.yml, the language file and the items, then `reload_success`.
  - anything else: `invalid_subcommand`. A missing permission gives `no_permission` (red).
- [x] Tab-complete: first word offers `reload`, `tool`, `give`, `help` (reload/give only with their permission,
  `tool`/`help` always), filtered by the lower-cased prefix; after `give` it offers online player names.
- [x] Permission nodes with default op: `commandgui.use`, `.tool`, `.give`, `.reload`, plus the undeclared
  `commandgui.bypass` (skips cooldowns; Bukkit treats an undeclared node as op-default).
- [x] Config `config.yml` and `lang/*.yml` (8 languages) are the original files: created from the shipped defaults when
  missing, read as they are. Keys: `language-file`, `verbose-mode`, `gui-size-mode` (`dynamic`: `(items/9+1)*9` slots,
  `fixed`: 54), `gui-tool`, `gui-items[]` with `slot`, `name`, `command`, `item`, `run-as-player`, `cooldown`,
  `verbose`, `lore`. `&` colour codes in names, lore and messages. `%player%`, `%name%`, `%time%` placeholders.
- [x] Menu title is `gui_title`; the window cannot be changed (no grabbing or putting items); every click in it is cancelled.
- [x] A click on an item: cooldown per player and slot (`cooldown_active`, red), verbose log line (`verbose_message`;
  item `verbose` overrides `verbose-mode`), then the command with `%player%` replaced, as the player or the console.
- [x] The tool is the `gui-tool` item named `tool_name` with lore `tool_description`; any right/left click with it
  in hand opens the menu instead of the normal action.
- [x] Console log lines: `Language file created: x`, `Loaded language file: x`, `Loaded GUI item: <name> at slot <n>`,
  `Invalid material ...`, `CommandGUI Plugin has been enabled.` / `disabled.`

## Differences inside

- **No update check.** The original asks the GitHub API for the latest release on start; this port makes no network
  calls (no `http.outbound` permission needed).
- **Item names** are looked up in Pumpkin's item registry (`Material.matchMaterial` equivalent: case-insensitive,
  optional `minecraft:`, spaces as underscores). Legacy Bukkit-only material names are not recognised.
- **gui-tool is resolved when the config loads**, so the "Material was null/empty. Using fallback" warning (the lobby
  config has no `gui-tool`) is logged once per load, not on every click or `/cg tool`.
- **Menu size above 54 slots** (more than 53 items) is cut at 54 (Pumpkin screens are 9x1 to 9x6). Bukkit would throw.
  An item whose `slot` lies outside the window is skipped with a warning (Bukkit throws).
- **Only clicks inside the menu run items.** The original compared `event.getSlot()`, which for a click in the player's
  own inventory is that inventory's index, so a named item in the player's slot 2 could run menu item 2. Not copied.
- **Which window is the menu** is tracked per player (opened by the plugin until the close event) because the click
  event carries no title to compare, unlike Bukkit's `getView().getTitle()`.
- **Tool detection** compares the held item's custom name with `tool_name` (serialised text components), not the
  display-name string.
- **Item with no free slot:** the tool goes to the first free storage slot; with a full inventory it is lost, as
  with Bukkit's `addItem` whose leftover the original ignores.
- **Permission node `commandgui:command`** (default allow) is added: Pumpkin needs a node to register a command with, and
  the original has none. It only lets everybody type `/cg`.
- Permission checks go to Pumpkin's permission manager. LuckPerms (a Bukkit plugin on the lobby) does not see this
  plugin's nodes: grants made with `lp` do not apply to the port (ops do pass, default is op).

## Server requirements

- Pumpkin built from `dev/Pumpkin` (plugin API v0.1: GUI, inventory click/close, player interact, commands).
- The build is unsigned and needs the data folder: in `pumpkin.toml`

  ```toml
  [plugins.overrides.CommandGUI]
  allow_unsigned = true
  allowed_permissions = ["fs.write.data"]   # config.yml and lang/ live in the plugin data folder
  ```

  (or confirm the permission prompt on the console once; the exact permission spelling is checked in step 7 of the port skill).
- The plugin data folder must hold the original `config.yml` and `lang/` to keep the live setup.

## Build

```
dev/tools/build-own.sh -p command-gui     # target wasm32-wasip2
```

Copy `Own/target/wasm32-wasip2/release/command_gui.wasm` into the server's `plugins/` folder.

## Tested

`dev/plugin-tests/tests/50-command-gui.js` (lobby). Not yet run: the tester runs it against the jar and the wasm.
