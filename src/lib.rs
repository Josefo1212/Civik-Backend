/// El crate como biblioteca, además de binario.
///
/// Existe para que los tests de integración en `tests/` puedan llamar al
/// código real (`shared::audit::write_audit_log`, `app::build_router` para
/// `tower::ServiceExt::oneshot`): un test no puede hacer `use` de los módulos
/// de un binario. Mientras `main.rs` declaraba los módulos con `mod`, todo lo
/// de acá era inaccesible desde `tests/`.
pub mod app;
pub mod config;
pub mod modules;
pub mod shared;
