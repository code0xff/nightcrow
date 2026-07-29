//! Browser-facing surface: the web viewer reads the same git data and drives the
//! same terminal sessions nightcrow works with, rendered as a DOM app rather
//! than a reflected terminal screen.

pub(crate) mod common;
pub(crate) mod viewer;
