//! El crate como biblioteca, además de binario.
//!
//! Existe para que los tests de integración en `tests/` puedan llamar al
//! código real (`shared::audit::write_audit_log`, `app::build_router` para
//! `tower::ServiceExt::oneshot`): un test no puede hacer `use` de los módulos
//! de un binario. Mientras `main.rs` declaraba los módulos con `mod`, todo lo
//! de acá era inaccesible desde `tests/`.
//!
//! `unwrap()`/`expect()` están prohibidos en todo el código de producción de
//! este crate (constitución / AGENTS: la política es fallar con un `Result`
//! propagado, nunca con panic). La ÚNICA excepción son los archivos de
//! arranque `main.rs` y `config/database.rs`, donde fallar rápido es la
//! política explícita y llevan su `#![allow(...)]` propio.

// Los inner attributes deben ir antes de cualquier doc comment del crate.
#![deny(clippy::unwrap_used, clippy::expect_used)]

pub mod app;
pub mod config;
pub mod modules;
pub mod shared;
