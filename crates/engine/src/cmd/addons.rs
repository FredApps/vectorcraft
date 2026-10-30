//! VectorCraft's built-in add-ons: extra tools beyond the standard menus, each a module of its own
//! with commands under `addon.<name>.*` (see `docs/mcp.md`).

mod cleanup;

use super::CommandSpec;

/// The commands of every built-in add-on.
pub fn specs() -> Vec<CommandSpec> {
    cleanup::specs()
}
