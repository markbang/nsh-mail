//! nsh-mail core: types shared with every client, plus (feature `engine`) the mail engine used by the
//! native app in local mode and by the cloud server.

pub mod types;
pub use types::*;

#[cfg(all(feature = "insecure-test-tls", not(debug_assertions)))]
compile_error!(
    "feature `insecure-test-tls` disables certificate checks and must never be built in release"
);

#[cfg(feature = "engine")]
mod engine;
#[cfg(feature = "engine")]
mod error;
#[cfg(feature = "engine")]
pub mod feed;
#[cfg(feature = "engine")]
mod imap;
#[cfg(feature = "engine")]
mod mime;
#[cfg(feature = "engine")]
pub mod oauth;
#[cfg(feature = "engine")]
pub mod provider;
#[cfg(feature = "engine")]
pub mod secrets;
#[cfg(feature = "engine")]
mod smtp;
#[cfg(feature = "engine")]
mod store;
#[cfg(feature = "engine")]
mod tls;

#[cfg(feature = "engine")]
pub use engine::{Engine, EngineOptions, StagedFile};
#[cfg(feature = "engine")]
pub use feed::{
    Feed, FeedCounts, FeedMessage, Inserted, NewFeedMessage, NewSource, Source, SourcePatch,
};
#[cfg(feature = "engine")]
pub use oauth::{OAuthApp, OAuthApps, OAuthGrant, OAuthTokens};
#[cfg(feature = "engine")]
pub use secrets::SecretStore;
#[cfg(feature = "engine")]
pub use store::{ImportCounts, LOCAL_USER, MailDb};
#[cfg(feature = "insecure-test-tls")]
pub use tls::accept_invalid_certs_for_tests;
