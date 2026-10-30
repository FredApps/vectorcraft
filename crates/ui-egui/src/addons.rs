//! VectorCraft's built-in add-ons in the interface: each has a panel and a submenu at the top of
//! Object › Plug-ins, and acts through its engine commands (`addon.<name>.*`).

pub mod cleanup;
pub mod color;

use crate::menus::Item;

/// The add-ons' submenus, then a separator: the head of Object › Plug-ins.
pub fn plugin_menu() -> Vec<Item> {
    vec![Item::Sub("Artwork Cleanup", cleanup::menu()), Item::Sub("Color Effects", color::menu()), Item::Sep]
}
