// La excepción explícita al `deny` de `lib.rs`: este archivo es el arranque,
// el único lugar (junto con `config/database.rs`) donde la política es fallar
// rápido con panic en vez de propagar un `Result` — la app no tiene sentido a
// medio levantar.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use civik_backend::{app, config};
use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    let filtro = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filtro).init();

    let pool = config::database::create_pool().await;
    let app = app::build_router(pool);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::info!("Servidor escuchando en {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
