//! Operating-system-adjacent services shared by application layers.

pub(crate) mod console;
pub(crate) mod fs;
mod link_target;
mod link_target_paths;
pub(crate) mod links;
pub(crate) mod logging;
pub(crate) mod paths;
pub(crate) mod self_replace;
pub(crate) mod signals;
pub(crate) mod threading;
