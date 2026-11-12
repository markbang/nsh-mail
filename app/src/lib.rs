//! nsh-mail's Rust side: the `Mail` interface TSX imports from
//! `nsh-mail/native` (docs/design.md §3), its backends and their types.

mod backend;
mod demo;
mod mail;
mod types;

use nsh_core::prelude::*;

pub use crate::{
    backend::{Backend, Events, MailError, MailFuture},
    demo::DemoBackend,
    mail::Mail,
    types::*,
};

/// What `App.tsx` imports from `nsh-mail/native`.
#[native_module("nsh-mail/native")]
fn native(registry: &mut Registry) {
    registry.module::<Mail>();
}
