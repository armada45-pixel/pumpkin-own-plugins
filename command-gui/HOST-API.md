# Host API wanted by CommandGUI

Nothing blocks the port. These exist in the WIT v0.2 files in `dev/Pumpkin` but not in v0.1, which is what
`pumpkin-plugin-api` is generated from today, so the port works around them:

| Wanted (v0.2 name) | Replaces (Bukkit) | Workaround now |
|---|---|---|
| `player-inventory.as-inventory().add-item(item) -> option<item-stack>` | `getInventory().addItem` | first free slot of storage 0..36 by hand, no stack merging (the tool is not stackable) |
| `player.perform-command(command) -> bool` | `Player#performCommand` | `server.execute-command(cmd, command-sender::player(player))` (v0.1, takes a `server::command-sender` variant) |

Open question for the main session: does the `pumpkin-plugin-api` crate move to WIT v0.2 (the dev servers' host
supports both)? Not needed for this port.
