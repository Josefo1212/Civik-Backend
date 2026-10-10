use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttendanceState {
    Presente,
    Ausente,
    Justificado,
}

#[derive(Debug, Error)]
pub enum ErrorAsistencia {
    #[error("No se puede registrar asistencia en evento CANCELLED")]
    EventoCancelado,
    #[error("Evento con asistencia existente: no se puede eliminar")]
    EventoConAsistencia,
}

pub fn state_from_str(s: &str) -> Option<AttendanceState> {
    match s {
        "PRESENTE" => Some(AttendanceState::Presente),
        "AUSENTE" => Some(AttendanceState::Ausente),
        "JUSTIFICADO" => Some(AttendanceState::Justificado),
        _ => None,
    }
}

pub fn state_to_str(s: AttendanceState) -> &'static str {
    match s {
        AttendanceState::Presente => "PRESENTE",
        AttendanceState::Ausente => "AUSENTE",
        AttendanceState::Justificado => "JUSTIFICADO",
    }
}
