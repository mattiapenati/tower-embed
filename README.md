# tower-embed

[![Latest Version](https://img.shields.io/crates/v/tower-embed.svg)](https://crates.io/crates/tower-embed)
[![Latest Version](https://docs.rs/tower-embed/badge.svg)](https://docs.rs/tower-embed)
![Apache 2.0 OR MIT licensed](https://img.shields.io/badge/license-Apache2.0%2FMIT-blue.svg)

This crate provides a [`tower`] service designed to provide embedded static
assets support for web application. This service includes the following HTTP features:

- Support for GET and HEAD requests
- `Content-Type` header generation based on file MIME type guessed from extension.
- `ETag` header generation and validation.
- `Last-Modified` header generation and validation.

In `debug` mode, assets are served directly from the filesystem to facilitate
rapid development. Both `ETag` and `Last-Modified` headers are not generated in
this mode.

The optional feature `astro` can be used to enable support for embedding
[Astro] project. In release mode the project is build and embedded as static
assets, while in debug mode the service acts as a proxy server pointing to the
development server.

## Example

```rust
use axum::Router;
use tower_embed::{EmbedExt, EmbedFolder, ServeEmbed};

#[derive(EmbedFolder)]
#[embed(folder = "assets")]
struct Assets;

#[tokio::main]
async fn main() {
    let assets = ServeEmbed::<Assets>::new().with_not_found(Assets::not_found_page("404.html"));
    let router = Router::new().fallback_service(assets);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080")
        .await
        .unwrap();
    axum::serve::serve(listener, router).await.unwrap();
}
```

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this crate by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.

[`tower`]: https://crates.io/crates/tower
[Astro]: https://astro.build/
