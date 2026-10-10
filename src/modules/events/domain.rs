//! Reglas puras del módulo `events` (CRUD manual).
//!
//! Sin `sqlx`, sin `async`, sin base de datos. Validaciones de entrada,
//! restricciones para cancelar/editar (RF-9, RF-10, RF-11).

use std::fmt;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ErrorDominio {
    #[error("El campo «fecha del evento» '{valor}' no es una fecha válida (use AAAA-MM-DD)")]
    FechaInvalida { valor: String },

    #[error("El campo «nombre del evento» no puede estar vacío")]
    NombreVacio,

    #[error("El tipo de evento '{valor}' no es válido")]
    TipoInvalido { valor: String },

    #[error("No se puede cancelar el evento: ya está CANCELLED")]
    YaCancelado,

    #[error("No se puede cancelar el evento: tiene asistencia registrada")]
    EventoConAsistenciaNoCancelable,

    #[error("No se puede editar el evento: ya está CANCELLED")]
    EditarCancelado,

    #[error("No se puede editar el evento: es de origen AUTO (solo se editan eventos MANUAL)")]
    EditarNoManual,

    #[error("No se puede editar el evento: tiene asistencia registrada")]
    EventoConAsistenciaNoEditable,

    #[error("No se puede cancelar el evento: debe estar SCHEDULED")]
    CancelarEstadoIncorrecto,

    #[error("La fecha del evento {fecha} no está dentro del rango del lapso")]
    FueraDelRango { fecha: chrono::NaiveDate },

    #[error(
        "La práctica regular manual del lapso debe caer en el día de práctica {dia_practica}, pero {fecha} es {dia_fecha}"
    )]
    PracticaRegularFueraDeDia {
        dia_practica: String,
        fecha: chrono::NaiveDate,
        dia_fecha: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipoEvento {
    PracticaRegular,
    PracticaExtra,
    Presentacion,
}

impl TipoEvento {
    pub fn as_str(&self) -> &'static str {
        match self {
            TipoEvento::PracticaRegular => "PRACTICA_REGULAR",
            TipoEvento::PracticaExtra => "PRACTICA_EXTRA",
            TipoEvento::Presentacion => "PRESENTACION",
        }
    }

    pub fn parse(valor: &str) -> Option<Self> {
        match valor.trim().to_uppercase().as_str() {
            "PRACTICA_REGULAR" => Some(TipoEvento::PracticaRegular),
            "PRACTICA_EXTRA" => Some(TipoEvento::PracticaExtra),
            "PRESENTACION" => Some(TipoEvento::Presentacion),
            _ => None,
        }
    }
}

impl fmt::Display for TipoEvento {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstadoEvento {
    Scheduled,
    Cancelled,
}

impl EstadoEvento {
    pub fn as_str(&self) -> &'static str {
        match self {
            EstadoEvento::Scheduled => "SCHEDULED",
            EstadoEvento::Cancelled => "CANCELLED",
        }
    }

    pub fn parse(texto: &str) -> Option<Self> {
        match texto {
            "SCHEDULED" => Some(EstadoEvento::Scheduled),
            "CANCELLED" => Some(EstadoEvento::Cancelled),
            _ => None,
        }
    }
}

impl fmt::Display for EstadoEvento {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct DatosCrearManual {
    pub event_date: chrono::NaiveDate,
    pub type_event: TipoEvento,
    pub event_name: String,
}

/// Normaliza y valida entrada de creación manual (RF-2).
pub fn preparar_creacion_manual(
    event_date: &str,
    type_event: &str,
    event_name: &str,
) -> Result<DatosCrearManual, ErrorDominio> {
    let event_name = event_name.trim();
    if event_name.is_empty() {
        return Err(ErrorDominio::NombreVacio);
    }
    let fecha = chrono::NaiveDate::parse_from_str(event_date.trim(), "%Y-%m-%d").map_err(|_| {
        ErrorDominio::FechaInvalida {
            valor: event_date.trim().to_string(),
        }
    })?;
    let tipo = TipoEvento::parse(type_event).ok_or_else(|| ErrorDominio::TipoInvalido {
        valor: type_event.to_string(),
    })?;
    Ok(DatosCrearManual {
        event_date: fecha,
        type_event: tipo,
        event_name: event_name.to_string(),
    })
}

#[derive(Debug, Clone)]
pub struct DatosEditarManual {
    pub event_date: chrono::NaiveDate,
    pub type_event: TipoEvento,
    pub event_name: String,
}

pub fn preparar_edicion_manual(
    event_date: &str,
    type_event: &str,
    event_name: &str,
) -> Result<DatosEditarManual, ErrorDominio> {
    let event_name = event_name.trim();
    if event_name.is_empty() {
        return Err(ErrorDominio::NombreVacio);
    }
    let fecha = chrono::NaiveDate::parse_from_str(event_date.trim(), "%Y-%m-%d").map_err(|_| {
        ErrorDominio::FechaInvalida {
            valor: event_date.trim().to_string(),
        }
    })?;
    let tipo = TipoEvento::parse(type_event).ok_or_else(|| ErrorDominio::TipoInvalido {
        valor: type_event.to_string(),
    })?;
    Ok(DatosEditarManual {
        event_date: fecha,
        type_event: tipo,
        event_name: event_name.to_string(),
    })
}

/// Reglas para cancelar: RF-9, RF-11.
pub fn puede_cancelarse(
    estado: EstadoEvento,
    tiene_asistencia: bool,
) -> Result<(), ErrorDominio> {
    if tiene_asistencia {
        return Err(ErrorDominio::EventoConAsistenciaNoCancelable);
    }
    match estado {
        EstadoEvento::Cancelled => Err(ErrorDominio::YaCancelado),
        EstadoEvento::Scheduled => Ok(()),
    }
}

/// Reglas para editar: RF-10, RF-11.
pub fn puede_editarse(
    estado: EstadoEvento,
    source_es_manual: bool,
    tiene_asistencia: bool,
) -> Result<(), ErrorDominio> {
    if tiene_asistencia {
        return Err(ErrorDominio::EventoConAsistenciaNoEditable);
    }
    if !source_es_manual {
        return Err(ErrorDominio::EditarNoManual);
    }
    match estado {
        EstadoEvento::Cancelled => Err(ErrorDominio::EditarCancelado),
        EstadoEvento::Scheduled => Ok(()),
    }
}

pub fn es_practica_regular_manual(type_event: TipoEvento, source_es_manual: bool) -> bool {
    matches!(type_event, TipoEvento::PracticaRegular) && source_es_manual
}

#[derive(Debug, Clone)]
pub struct DatosGenerarAuto {
    pub fechas: Vec<chrono::NaiveDate>,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum ErrorGeneracion {
    #[error("El lapso está cerrado o no permite generación")]
    LapsoNoGenerable,
    #[error("No hay fechas de práctica válidas")]
    SinFechasValidas,
}

/// Determina si se puede generar prácticas automáticas para el lapso
pub fn puede_generar_auto(lapso_status: Option<&str>, _has_events: bool) -> bool {
    // Generar si está OPEN o PLANEADO (no CLOSED). No duplicar innecesariamente - lógica en service.
    match lapso_status {
        Some("CLOSED") => false,
        Some("OPEN") | Some("PLANEADO") | None => true,
        _ => true,
    }
}
