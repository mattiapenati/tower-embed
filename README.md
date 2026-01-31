# tower-embed

[![Latest Version](https://img.shields.io/crates/v/tower-embed.svg)](https://crates.io/crates/tower-embed)
[![Latest Version](https://docs.rs/tower-embed/badge.svg)](https://docs.rs/tower-embed)
![Apache 2.0 OR MIT licensed](https://img.shields.io/badge/license-Apache2.0%2FMIT-blue.svg)

**tower-embed** is a [`tower`] service that efficiently serves embedded static assets in Rust web applications. It provides a production-ready solution for bundling and serving static files (HTML, CSS, JavaScript, images, etc.) directly within your compiled binary, eliminating the need for external file deployments.

## Features

This service includes comprehensive HTTP features for optimal asset delivery:

- **HTTP Method Support**: GET and HEAD requests
- **Smart Content Detection**: Automatic `Content-Type` header generation based on file MIME type detection from extensions
- **Efficient Caching**: 
  - `ETag` header generation and validation for strong cache control
  - `Last-Modified` header generation and validation for conditional requests
- **Development-Friendly**: In `debug` mode, assets are served directly from the filesystem for rapid iteration without recompilation (caching headers are disabled in this mode)
- **Zero Dependencies at Runtime**: All assets are embedded in the binary at compile time in release builds

## Example

```rust
use axum::Router;
use tower_embed::Embed;

#[derive(Embed)]
#[embed(folder = "assets")]
struct Assets;

#[tokio::main]
async fn main() {
    let assets = tower_embed::ServeEmbed::<Assets>::new();
    let router = Router::new().fallback_service(assets);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080")
        .await
        .unwrap();
    axum::serve::serve(listener, router).await.unwrap();
}
```

This creates a Tower service that serves all files from the `assets` directory. In release builds, the files are embedded in the binary at compile time. In debug builds, files are read from the filesystem for faster development iteration.

## Use Cases

**tower-embed** is ideal for:

- **Single Binary Deployment**: Ship web applications as a single executable with all assets included
- **Microservices**: Serve UI assets alongside API endpoints without external file management
- **Desktop Applications**: Embed web UIs in desktop apps built with frameworks like Tauri
- **CLI Tools**: Include documentation or web-based dashboards in command-line tools
- **Embedded Systems**: Deploy web interfaces to resource-constrained devices with minimal footprint

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this crate by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.

[`tower`]: https://crates.io/crates/tower
