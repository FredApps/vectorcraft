//! Geometry for VectorCraft's built-in add-ons, one module per add-on, with the kernels they share.
//!
//! - [`keep_shape`]: remove anchors while refitting their neighbours' handles, so the outline keeps
//!   its shape within a tolerance (shared by every add-on that takes anchors away).
//! - [`cleanup`]: Artwork Cleanup's path repairs (collinear, duplicate and near anchors, tiny
//!   handles, nearly closed paths) and Rejoin Path Pieces.

pub mod cleanup;
pub mod keep_shape;

pub use cleanup::{
    Rejoined, close_nearly_closed, merge_duplicate_anchors, near_point_groups, rejoin, remove_collinear_anchors, retract_tiny_handles,
};
pub use keep_shape::{SHARP_CORNER_DEG, remove_anchor_keep_shape, simplify_keep_shape};
