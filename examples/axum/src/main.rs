use axum::Router;
use tower_embed::{Embed, ServeEmbed};

#[derive(Embed)]
#[embed(folder = "assets")]
struct Assets;

#[tokio::main]
async fn main() {
    let assets = ServeEmbed::<Assets>::new();
    let router = Router::new().fallback_service(assets);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080")
        .await
        .unwrap();
    axum::serve::serve(listener, router).await.unwrap();
}
