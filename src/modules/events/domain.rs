//! Reglas puras del módulo `events` (CRUD manual + generación automática y
//! reprogramación de la fase 6).
//!
//! Sin `sqlx`, sin `async`, sin base de datos. Validaciones de entrada,
//! restricciones para cancelar/editar (RF-9, RF-10, RF-11) y las reglas de
//! `generar_practicas_auto` / `reprogramar_practice_day` (RF-14 a RF-18).

use crate::modules::lapses::domain::{DiaSemana, EstadoLapso};
use chrono::{Datelike, NaiveDate};
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
pub fn puede_cancelarse(estado: EstadoEvento, tiene_asistencia: bool) -> Result<(), ErrorDominio> {
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

// ---------------------------------------------------------------------------
// Fase 6: generación automática y reprogramación (RF-14 a RF-18)
// ---------------------------------------------------------------------------
// El catálogo de días y estados viene de `lapses::domain` (`DiaSemana`,
// `EstadoLapso`): es la fuente única, y acá no se re-parsea ningún enum de la
// base. Este módulo agrega solo lo que es calendario (`weekday_de`) y las
// reglas de decisión, todo puro.
// ---------------------------------------------------------------------------

/// Un lapso permite generar prácticas mientras no esté cerrado (RF-14).
///
/// Tanto `PLANNED` como `ACTIVE` lo permiten; el `CLOSED` es terminal (regla
/// 4 de la base) y no admite datos nuevos. El `Some(_)` imposible (un texto
/// que no es enum) no existe: `EstadoLapso` solo construye estados reales.
pub fn puede_generar_auto(estado: &EstadoLapso) -> bool {
    estado != &EstadoLapso::Cerrado
}

/// La contraparte en `chrono` del `week_day_of` de la base (ver init): mapea
/// el enum `week_day` (MONDAY…SUNDAY) al `Weekday` de la fecha. Es lo único
/// calendárico que necesita `events` y aqui no se duplica el catálogo.
pub fn weekday_de(dia: DiaSemana) -> chrono::Weekday {
    match dia {
        DiaSemana::Lunes => chrono::Weekday::Mon,
        DiaSemana::Martes => chrono::Weekday::Tue,
        DiaSemana::Miercoles => chrono::Weekday::Wed,
        DiaSemana::Jueves => chrono::Weekday::Thu,
        DiaSemana::Viernes => chrono::Weekday::Fri,
        DiaSemana::Sabado => chrono::Weekday::Sat,
        DiaSemana::Domingo => chrono::Weekday::Sun,
    }
}

/// Todas las fechas del lapso que caen en el día de práctica (RF-14).
///
/// Recorre `[inicio, fin]` y conserva las fechas cuyo `weekday` coincide con
/// `dia`. No mira el reloj de la máquina a propósito: un lapso capturado
/// tarde debe poder generar las prácticas de los días ya transcurridos
/// (constitución, principio 4: la evidencia manda, no la fecha).
pub fn fechas_de_practica(inicio: NaiveDate, fin: NaiveDate, dia: DiaSemana) -> Vec<NaiveDate> {
    let objetivo = weekday_de(dia);
    let mut fechas = Vec::new();
    let mut fecha = inicio;
    while fecha <= fin {
        if fecha.weekday() == objetivo {
            fechas.push(fecha);
        }
        let Some(siguiente) = fecha.succ_opt() else {
            break;
        };
        fecha = siguiente;
    }
    fechas
}

/// La primera fecha estrictamente posterior a `desde`, menor o igual a `fin`,
/// cuyo `weekday` es el día objetivo (RF-16).
///
/// «Estrictamente posterior» importa: al reprogramar, un evento que ya cayera
/// en el nuevo día no debe quedarse en su sitio por reubicarse a sí mismo;
/// avanza a la siguiente semana. Devuelve `None` si no cabe una ocurrencia
/// del nuevo día dentro del lapso (RF-18).
pub fn proxima_fecha_practica(
    desde: NaiveDate,
    fin: NaiveDate,
    dia: DiaSemana,
) -> Option<NaiveDate> {
    let objetivo = weekday_de(dia);
    let mut fecha = desde.succ_opt()?;
    while fecha <= fin {
        if fecha.weekday() == objetivo {
            return Some(fecha);
        }
        let Some(siguiente) = fecha.succ_opt() else {
            break;
        };
        fecha = siguiente;
    }
    None
}

/// Un evento pendiente es candidato a reprogramarse si es una práctica
/// regular (AUTO o MANUAL), sigue `SCHEDULED` y no tiene asistencia (RF-16).
///
/// La asistencia congela el evento (regla 9): esa práctica ya ocurrió y su
/// fecha es historia. Los `CANCELLED` (regla 11) tampoco se mueven. Las
/// prácticas extra y las presentaciones no se anclan al día de práctica
/// (fase 5, RF-4), así que no se tocan.
pub fn puede_reprogramarse_evento(
    estado: Option<&str>,
    source: Option<&str>,
    type_event: Option<&str>,
    tiene_asistencia: bool,
) -> bool {
    !tiene_asistencia
        && matches!(estado, Some("SCHEDULED"))
        && matches!(source, Some("AUTO") | Some("MANUAL"))
        && matches!(type_event, Some("PRACTICA_REGULAR"))
}

// Los tests de unidad pueden usar `unwrap()`: el `deny` de `lib.rs` protege
// el código de producción, y en un assert un panic es el fallo esperado.
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn fecha(anio: i32, mes: u32, dia: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(anio, mes, dia).unwrap()
    }

    // --- puede_generar_auto (RF-14, RF-19) ---

    #[test]
    fn se_genera_solo_si_el_lapso_no_esta_cerrado() {
        assert!(puede_generar_auto(&EstadoLapso::Planeado));
        assert!(puede_generar_auto(&EstadoLapso::Activo));
        assert!(!puede_generar_auto(&EstadoLapso::Cerrado));
    }

    // --- weekday_de: la contraparte del enum de la base ---

    #[test]
    fn cada_dia_del_catalogo_mapea_a_su_weekday() {
        assert_eq!(weekday_de(DiaSemana::Lunes), chrono::Weekday::Mon);
        assert_eq!(weekday_de(DiaSemana::Martes), chrono::Weekday::Tue);
        assert_eq!(weekday_de(DiaSemana::Miercoles), chrono::Weekday::Wed);
        assert_eq!(weekday_de(DiaSemana::Jueves), chrono::Weekday::Thu);
        assert_eq!(weekday_de(DiaSemana::Viernes), chrono::Weekday::Fri);
        assert_eq!(weekday_de(DiaSemana::Sabado), chrono::Weekday::Sat);
        assert_eq!(weekday_de(DiaSemana::Domingo), chrono::Weekday::Sun);
    }

    // --- fechas_de_practica (RF-14) ---

    #[test]
    fn lista_los_jueves_del_rango() {
        let fechas = fechas_de_practica(fecha(2026, 9, 1), fecha(2026, 9, 30), DiaSemana::Jueves);

        assert_eq!(
            fechas,
            vec![
                fecha(2026, 9, 3),
                fecha(2026, 9, 10),
                fecha(2026, 9, 17),
                fecha(2026, 9, 24),
            ]
        );
    }

    #[test]
    fn incluye_los_bordes_cuando_caen_en_el_dia() {
        let fechas = fechas_de_practica(fecha(2026, 9, 3), fecha(2026, 9, 3), DiaSemana::Jueves);
        assert_eq!(fechas, vec![fecha(2026, 9, 3)]);

        let fechas = fechas_de_practica(fecha(2026, 9, 3), fecha(2026, 9, 10), DiaSemana::Jueves);
        assert_eq!(fechas, vec![fecha(2026, 9, 3), fecha(2026, 9, 10)]);
    }

    #[test]
    fn sin_ocurrencias_devuelve_vacio() {
        // Del viernes 4 al miércoles 9 no hay ningún jueves.
        let fechas = fechas_de_practica(fecha(2026, 9, 4), fecha(2026, 9, 9), DiaSemana::Jueves);
        assert!(fechas.is_empty());
    }

    #[test]
    fn no_mira_el_reloj_la_misma_lista_para_dias_pasados() {
        // Septiembre de 2026 ya pasó al momento de escribir esto, y sin
        // embargo el generador ve todas las fechas: la evidencia manda, no
        // la fecha.
        let fechas = fechas_de_practica(fecha(2026, 9, 1), fecha(2026, 9, 30), DiaSemana::Jueves);
        assert_eq!(fechas.len(), 4);
    }

    // --- proxima_fecha_practica (RF-16, RF-18) ---

    #[test]
    fn la_proxima_es_estrictamente_posterior() {
        // Desde un jueves, la "próxima" es el jueves de la semana siguiente,
        // no el mismo día.
        assert_eq!(
            proxima_fecha_practica(fecha(2026, 9, 3), fecha(2026, 9, 30), DiaSemana::Jueves),
            Some(fecha(2026, 9, 10))
        );
    }

    #[test]
    fn avanza_hasta_el_siguiente_dia_objetivo() {
        // Desde un viernes, el próximo jueves es el 10.
        assert_eq!(
            proxima_fecha_practica(fecha(2026, 9, 4), fecha(2026, 9, 30), DiaSemana::Jueves),
            Some(fecha(2026, 9, 10))
        );
    }

    #[test]
    fn sin_cupo_en_el_lapso_devuelve_none() {
        // El próximo jueves después del 29 de septiembre es el 1 de octubre,
        // fuera del rango.
        assert_eq!(
            proxima_fecha_practica(fecha(2026, 9, 29), fecha(2026, 9, 30), DiaSemana::Jueves),
            None
        );
        // Con `desde == fin` no queda margen.
        assert_eq!(
            proxima_fecha_practica(fecha(2026, 9, 30), fecha(2026, 9, 30), DiaSemana::Jueves),
            None
        );
    }

    // --- puede_reprogramarse_evento (RF-16) ---

    #[test]
    fn mueve_solo_pendientes_regulares() {
        assert!(puede_reprogramarse_evento(
            Some("SCHEDULED"),
            Some("AUTO"),
            Some("PRACTICA_REGULAR"),
            false
        ));
        assert!(puede_reprogramarse_evento(
            Some("SCHEDULED"),
            Some("MANUAL"),
            Some("PRACTICA_REGULAR"),
            false
        ));
    }

    #[test]
    fn un_evento_con_asistencia_no_se_mueve() {
        assert!(!puede_reprogramarse_evento(
            Some("SCHEDULED"),
            Some("AUTO"),
            Some("PRACTICA_REGULAR"),
            true
        ));
    }

    #[test]
    fn un_cancelado_no_se_mueve() {
        assert!(!puede_reprogramarse_evento(
            Some("CANCELLED"),
            Some("AUTO"),
            Some("PRACTICA_REGULAR"),
            false
        ));
    }

    #[test]
    fn extra_y_presentacion_no_se_mueven() {
        assert!(!puede_reprogramarse_evento(
            Some("SCHEDULED"),
            Some("MANUAL"),
            Some("PRACTICA_EXTRA"),
            false
        ));
        assert!(!puede_reprogramarse_evento(
            Some("SCHEDULED"),
            Some("MANUAL"),
            Some("PRESENTACION"),
            false
        ));
    }

    #[test]
    fn datos_ausentes_no_se_mueven() {
        assert!(!puede_reprogramarse_evento(None, None, None, false));
    }
}
