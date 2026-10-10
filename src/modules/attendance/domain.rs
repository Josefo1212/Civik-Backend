//! Reglas puras del módulo `attendance`.
//!
//! Sin `sqlx`, sin `async`, sin base de datos (hexagonal parcial, plan §2).
//! Dos responsabilidades, ambas puras:
//!
//! 1. El catálogo de estados (`AttendanceState`) — parseo y forma canónica —
//!    y su validación de entrada (RF-5).
//! 2. `puede_escribir_asistencia` decide si un evento admite escritura de
//!    asistencia según el estado del evento y el del lapso (RF-2 y RF-3).
//!
//! Los ids van en newtypes (`EventId`, `StudentId`): `attendance` combina los
//! dos tipos de id en las mismas firmas, así que pasar `id_student` donde va
//! `id_event` deja de compilar (AGENTS.md, RNF-2).
//!
//! La base ya protege las invariantes duras con sus triggers (`CANCELLED`,
//! `CLOSED`, no borrar, no cambiar identidad). Estas funciones producen el
//! mensaje en español y evitan llegar al trigger con un `23514` que la API no
//! sabría traducir.

use std::fmt;
use thiserror::Error;
use uuid::Uuid;

/// Identificador de un evento (`events.id`), como newtype.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EventId(pub Uuid);

/// Identificador de un estudiante (`students.id`), como newtype.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StudentId(pub Uuid);

impl From<Uuid> for EventId {
    fn from(valor: Uuid) -> Self {
        EventId(valor)
    }
}

impl From<EventId> for Uuid {
    fn from(valor: EventId) -> Self {
        valor.0
    }
}

impl From<Uuid> for StudentId {
    fn from(valor: Uuid) -> Self {
        StudentId(valor)
    }
}

impl From<StudentId> for Uuid {
    fn from(valor: StudentId) -> Self {
        valor.0
    }
}

impl fmt::Display for EventId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Display for StudentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// El estado de una fila de asistencia, derivado de `attendance_state` de la
/// base.
///
/// El parseo es **literal** (mayúsculas exactas): el `CAST` de Postgres a su
/// enum tampoco acepta otra forma, así que `"presente"` es una entrada
/// inválida y responde `400` (RF-5), no una variante a medio normalizar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttendanceState {
    Presente,
    Ausente,
    Justificado,
}

impl AttendanceState {
    /// Parsea el estado tal como llega y como lo espera la base.
    pub fn parse(texto: &str) -> Option<AttendanceState> {
        match texto {
            "PRESENTE" => Some(AttendanceState::Presente),
            "AUSENTE" => Some(AttendanceState::Ausente),
            "JUSTIFICADO" => Some(AttendanceState::Justificado),
            _ => None,
        }
    }

    /// La forma canónica que la base entiende (y que devuelve `state::text`).
    pub fn as_str(&self) -> &'static str {
        match self {
            AttendanceState::Presente => "PRESENTE",
            AttendanceState::Ausente => "AUSENTE",
            AttendanceState::Justificado => "JUSTIFICADO",
        }
    }
}

/// Los casos que la API distingue y la base rechaza con un `23514` genérico.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ErrorAsistencia {
    /// El texto no corresponde a ningún estado de `attendance_state` (`400`).
    #[error(
        "El estado de asistencia '{0}' no es válido. Admitidos: PRESENTE, AUSENTE, JUSTIFICADO"
    )]
    EstadoInvalido(String),

    /// El evento está `CANCELLED` (`409`, regla 7).
    #[error("No se puede registrar asistencia en un evento cancelado")]
    EventoCancelado,

    /// El lapso del evento está `CLOSED` (`409`, regla 4).
    #[error("No se puede registrar asistencia en un lapso cerrado")]
    LapsoCerrado,

    /// Se intenta registrar asistencia nueva a un estudiante dado de baja
    /// (`409`, RF-6). Corregir su asistencia existente sí se permite.
    #[error("No se puede registrar asistencia nueva a un estudiante dado de baja")]
    EstudianteInactivo,
}

/// Valida que `texto` sea un estado de asistencia (RF-5).
pub fn validar_state(texto: &str) -> Result<AttendanceState, ErrorAsistencia> {
    AttendanceState::parse(texto).ok_or_else(|| ErrorAsistencia::EstadoInvalido(texto.to_string()))
}

/// ¿Admite escritura de asistencia un evento en `estado_evento` dentro de un
/// lapso en `estado_lapso`? (RF-2, RF-3).
///
/// Se toman `Option<&str>` en vez de los enums de `events`/`lapses` por la
/// misma razón que `events::domain::puede_reprogramarse_evento`: el estado
/// llega como texto de la base (`status::text`) y un `Option` deja explícito
/// el caso ausente sin un `unwrap`. Un texto desconocido no bloquea: la base
/// es la que decide con su trigger, y el dominio no inventa restricciones.
///
/// El orden importa (igual que el trigger `check_attendance_guards`): primero
/// `CANCELLED`, después `CLOSED`, para que cada causa se aísle.
pub fn puede_escribir_asistencia(
    estado_evento: Option<&str>,
    estado_lapso: Option<&str>,
) -> Result<(), ErrorAsistencia> {
    if estado_evento == Some("CANCELLED") {
        return Err(ErrorAsistencia::EventoCancelado);
    }
    if estado_lapso == Some("CLOSED") {
        return Err(ErrorAsistencia::LapsoCerrado);
    }
    Ok(())
}

// Los tests de unidad pueden usar `unwrap()`: el `deny` de `lib.rs` protege
// el código de producción, y en un assert un panic es el fallo esperado.
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    // --- Catálogo de estados (RF-5) ---

    #[test]
    fn los_tres_estados_se_parsean_en_forma_canonica() {
        assert_eq!(
            AttendanceState::parse("PRESENTE"),
            Some(AttendanceState::Presente)
        );
        assert_eq!(
            AttendanceState::parse("AUSENTE"),
            Some(AttendanceState::Ausente)
        );
        assert_eq!(
            AttendanceState::parse("JUSTIFICADO"),
            Some(AttendanceState::Justificado)
        );
    }

    #[test]
    fn el_parseo_es_literal_y_rechaza_otras_formas() {
        // La base tampoco aceptaría minúsculas: no se normaliza de más.
        for invalido in ["presente", "Presente", "", "PRESENT", "AUSENTES", "otro"] {
            assert_eq!(AttendanceState::parse(invalido), None, "texto: {invalido}");
        }
    }

    #[test]
    fn cada_estado_vuelve_a_su_texto() {
        for estado in [
            AttendanceState::Presente,
            AttendanceState::Ausente,
            AttendanceState::Justificado,
        ] {
            assert_eq!(
                AttendanceState::parse(estado.as_str()),
                Some(estado),
                "roundtrip de {}",
                estado.as_str()
            );
        }
    }

    #[test]
    fn validar_state_devuelve_el_estado_o_un_error() {
        assert_eq!(validar_state("AUSENTE"), Ok(AttendanceState::Ausente));
        assert_eq!(
            validar_state("nada"),
            Err(ErrorAsistencia::EstadoInvalido("nada".to_string()))
        );
    }

    #[test]
    fn el_error_de_estado_nombra_el_valor_y_los_admitidos() {
        let error = validar_state("xxx").unwrap_err();
        let mensaje = error.to_string();

        assert!(mensaje.contains("xxx"), "mensaje: {mensaje}");
        assert!(mensaje.contains("PRESENTE"), "mensaje: {mensaje}");
        assert!(mensaje.contains("JUSTIFICADO"), "mensaje: {mensaje}");
    }

    // --- Elegibilidad de escritura (RF-2, RF-3) ---

    #[test]
    fn un_evento_cancelado_no_admite_escritura() {
        assert_eq!(
            puede_escribir_asistencia(Some("CANCELLED"), Some("ACTIVE")),
            Err(ErrorAsistencia::EventoCancelado)
        );
    }

    #[test]
    fn un_lapso_cerrado_no_admite_escritura() {
        assert_eq!(
            puede_escribir_asistencia(Some("SCHEDULED"), Some("CLOSED")),
            Err(ErrorAsistencia::LapsoCerrado)
        );
    }

    #[test]
    fn el_evento_cancelado_manda_sobre_el_lapso_cerrado() {
        // Igual que el trigger: primero CANCELLED, después CLOSED.
        assert_eq!(
            puede_escribir_asistencia(Some("CANCELLED"), Some("CLOSED")),
            Err(ErrorAsistencia::EventoCancelado)
        );
    }

    #[test]
    fn un_evento_programado_en_lapso_abierto_se_permite() {
        for lapso in ["PLANNED", "ACTIVE"] {
            assert!(
                puede_escribir_asistencia(Some("SCHEDULED"), Some(lapso)).is_ok(),
                "lapso: {lapso}"
            );
        }
    }

    #[test]
    fn un_estado_ausente_no_bloquea() {
        // Un texto que no está no es CANCELLED ni CLOSED: la base decide.
        assert!(puede_escribir_asistencia(None, None).is_ok());
        assert!(puede_escribir_asistencia(Some("SCHEDULED"), None).is_ok());
    }

    // --- Newtypes (RNF-2) ---

    #[test]
    fn los_newtypes_conservan_el_uuid() {
        let id = Uuid::new_v4();
        assert_eq!(EventId::from(id).0, id);
        assert_eq!(StudentId::from(id).0, id);
        assert_eq!(Uuid::from(EventId(id)), id);
        assert_eq!(Uuid::from(StudentId(id)), id);
    }
}
