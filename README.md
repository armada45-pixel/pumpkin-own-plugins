# Own: Pumpkin plugins ported to Rust

Rust ports of the Bukkit/Paper plugins our network runs, as [Pumpkin](https://github.com/Pumpkin-MC/Pumpkin) Wasm
plugins. Each port keeps everything players and admins see (commands, permissions, messages, config and data
files) the same as the original. Its README lists what works differently inside.

| Plugin | Original | License |
|---|---|---|
| [inventory-totem](inventory-totem/) | [InventoryTotem](https://github.com/Mobilestars/InventoryTotem) by Mobilestars | Apache-2.0 |
| [anvil-unlocker](anvil-unlocker/) | [AnvilUnlocker](https://github.com/cloudnode-pro/AnvilUnlocker) by Jikoo, Cloudnode | Unlicense |
| [craftable-inv-frames](craftable-inv-frames/) | [craftableinvframes](https://codeberg.org/WasteLandOrigin/CraftableInvFrames) by Techdoodle, SpiritOTHawk | MIT AND GPL-3.0-or-later |
| [command-gui](command-gui/) | [CommandGUI](https://github.com/Alfie51m/CommandGUI) by Alfie51m | Apache-2.0 |
| [click-mobs](click-mobs/) | [ClickMobs](https://github.com/Clickism/ClickMobs) by Clickism | GPL-3.0-only |

Each plugin folder has its own license, which follows the original plugin's license.

Build all: `cargo build --release` (needs the `wasm32-wasip2` Rust target).
