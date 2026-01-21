use axum::Router;
use tower_embed::rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "assets"]
#[crate_path = "tower_embed::rust_embed"]
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
