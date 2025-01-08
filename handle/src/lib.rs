#![feature(array_try_from_fn)]
#![feature(sync_unsafe_cell)]

mod handle;
pub use handle::*;

mod signature;
pub use signature::*;

mod pattern;

pub use pattern::*;
pub use vtd_libum::{
    protocol::command::{
        KeyboardState,
        MouseState,
    },
    InterfaceError,
};
