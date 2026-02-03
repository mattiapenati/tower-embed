//! This crate provides a [`tower`] service designed to provide embedded static
//! assets support for web application. This service includes the following HTTP features:
//!
//! - Support for GET and HEAD requests
//! - `Content-Type` header generation based on file MIME type guessed from extension.
//! - `ETag` header generation and validation.
//! - `Last-Modified` header generation and validation.
//! - Customizable 404 page.
//!
//! In `debug` mode, assets are served directly from the filesystem to facilitate rapid
//! development. Both `ETag` and `Last-Modified` headers are not generated in this mode.
//!
//! # Usage
//!
//! ```no_run
//! use axum::Router;
//! use tower_embed::{EmbedExt, EmbedFolder, ServeEmbed};
//!
//! #[derive(Embed)]
//! #[embed(folder = "assets")]
//! struct Assets;
//!
//! #[tokio::main]
//! async fn main() {
//!     let assets = ServeEmbed::<Assets>::new().with_not_found(Assets::not_found_page("404.html"));
//!     let router = Router::new().fallback_service(assets);
//!
//!     let listener = tokio::net::TcpListener::bind("127.0.0.1:8080")
//!         .await
//!         .unwrap();
//!     axum::serve::serve(listener, router).await.unwrap();
//! }
//! ```
//!
//! Please see the [examples] directory for working examples.
//!
//! [`tower`]: https://crates.io/crates/tower
//! [examples]: https://github.com/mattiapenati/tower-embed/tree/main/examples

#[cfg(not(feature = "tokio"))]
compile_error!("Only tokio runtime is supported, and it is required to use `tower-embed`.");

#[doc(inline)]
pub use tower_embed_impl::Embed;

#[doc(hidden)]
pub use tower_embed_core as core;

#[doc(inline)]
pub use tower_embed_core::{Embed, EmbedExt, EmbedFolder, ServeEmbed};
