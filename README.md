# Own: Pumpkin plugins ported to Rust

Rust ports of the Bukkit/Paper plugins our network runs, as [Pumpkin](https://github.com/Pumpkin-MC/Pumpkin) Wasm
plugins. Each port keeps everything players and admins see (commands, permissions, messages, config and data
files) the same as the original. Its README lists what works differently inside.

| Plugin | Original | License |
|---|---|---|
| [inventory-totem](inventory-totem/) | [InventoryTotem](https://github.com/Mobilestars/InventoryTotem) by Mobilestars | Apache-2.0 |

Each plugin folder has its own license, which follows the original plugin's license.

Build all: `cargo build --release` (needs the `wasm32-wasip2` Rust target).
