//! nsh-mail's Rust side: the `Mail` interface TSX imports from
//! `nsh-mail/native` (docs/design.md §3) and its backends.

mod attachments;
mod backend;
mod cloud;
#[cfg(debug_assertions)]
mod demo;
mod layout;
#[cfg(not(target_arch = "wasm32"))]
mod local;
mod mail;
mod push;
mod types;

use nsh_core::prelude::*;

pub use crate::{
    backend::{Backend, Events, MailFuture},
    mail::Mail,
    push::{Push, TokenSink, on_token},
    types::Js,
};

/// What `App.tsx` imports from `nsh-mail/native`.
#[native_module("nsh-mail/native")]
fn native(registry: &mut Registry) {
    registry.module::<Mail>();
    registry.module::<Push>();
}
