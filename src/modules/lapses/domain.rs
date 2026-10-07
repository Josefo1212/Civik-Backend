//! Reglas puras del módulo `lapses`.
//!
//! Funciones sin `sqlx`, sin `async`, sin base de datos (hexagonal parcial,
//! plan §2). Dos responsabilidades, ambas puras:
//!
//! 1. `preparar_entrada` valida y arma los datos del alta/edición — el único
//!    punto de validación, igual que el de `sections` (RF-2, RF-8).
//! 2. `validar_transicion` avala el paso de estado de una operación según el
//!    ciclo `PLANNED → ACTIVE → CLOSED` que fijó la clarificación, y
//!    `validar_borrado_permitido` la excepción constitucional del borrado.
//!
//! La base garantiza las invariantes duras ("no se edita un combo CLOSED", "un
//! solo ACTIVE", "sin solape", "no se borra historial") con triggers e índices;
//! este dominio produce el **mensaje en español** para los casos que la base
//! rechaza con un `23514` de trigger sin constraint que la API no podría
//! traducir. El estado se lee de la base como texto (`status::text`) y se
//! re-parse acá: los enums de Postgres no los mapea sqlx en compilación.

use chrono::NaiveDate;
use std::fmt;
use thiserror::Error;

/// Máximo del `lapses.school_year` admitido por el dominio (spec 004, RF-2).
pub const ANIO_MINIMO: i32 = 2000;
pub const ANIO_MAXIMO: i32 = 2100;

/// Número de lapso válido (constraint `lapses_number_between_1_and_3` de la
/// base, que el dominio anticipa para dar un mensaje claro).
pub const NUMERO_MINIMO: i16 = 1;
pub const NUMERO_MAXIMO: i16 = 3;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ErrorDominio {
    #[error("El campo «año escolar» no es válido: debe ser un año entre {ANIO_MINIMO} y {ANIO_MAXIMO}")]
    AnioFueraDeRango,

    #[error("El campo «número de lapso» no es válido: debe ser {NUMERO_MINIMO}, {NUMERO_MAXIMO}. ")]
    NumeroFueraDeRango,

    #[error("La {campo} '{valor}' no es una fecha válida (use AAAA-MM-DD)")]
    FechaInvalida { campo: &'static str, valor: String },

    #[error("La fecha de fin es anterior a la fecha de inicio")]
    FechasInvertidas,

    #[error("El campo «día de práctica» '{dia}' no es válido. Días admitidos: {}", lista_dias())]
    DiaInvalido { dia: String },

    #[error("No se puede {operacion}: el lapso debe estar {esperado}, pero está {actual}")]
    EstadoIncorrecto {
        operacion: &'static str,
        esperado: &'static str,
        actual: String,
    },

    #[error("No se puede eliminar: el lapso terminó el {fecha} y ya forma parte del historial")]
    LapsoTerminado { fecha: NaiveDate },
}

/// Los siete días de la semana de `week_day` (`MONDAY`…`SUNDAY`).
///
/// Se guardan en mayúsculas, como el enum de la base. La entrada se normaliza
/// (recorte + mayúsculas) para que `" thursday "` valga `THURSDAY` (mismo
/// espíritu que `Grado`/`Turno` de la 002).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiaSemana {
    Lunes,
    Martes,
    Miercoles,
    Jueves,
    Viernes,
    Sabado,
    Domingo,
}

impl DiaSemana {
    /// La lista completa, ordenada de lunes a domingo: única fuente de verdad
    /// para la validación y el mensaje del error.
    pub const TODOS: [DiaSemana; 7] = [
        DiaSemana::Lunes,
        DiaSemana::Martes,
        DiaSemana::Miercoles,
        DiaSemana::Jueves,
        DiaSemana::Viernes,
        DiaSemana::Sabado,
        DiaSemana::Domingo,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            DiaSemana::Lunes => "MONDAY",
            DiaSemana::Martes => "TUESDAY",
            DiaSemana::Miercoles => "WEDNESDAY",
            DiaSemana::Jueves => "THURSDAY",
            DiaSemana::Viernes => "FRIDAY",
            DiaSemana::Sabado => "SATURDAY",
            DiaSemana::Domingo => "SUNDAY",
        }
    }
}

impl<'a> TryFrom<&'a str> for DiaSemana {
    type Error = ErrorDominio;

    fn try_from(valor: &'a str) -> Result<Self, Self::Error> {
        match valor.trim().to_uppercase().as_str() {
            "MONDAY" => Ok(DiaSemana::Lunes),
            "TUESDAY" => Ok(DiaSemana::Martes),
            "WEDNESDAY" => Ok(DiaSemana::Miercoles),
            "THURSDAY" => Ok(DiaSemana::Jueves),
            "FRIDAY" => Ok(DiaSemana::Viernes),
            "SATURDAY" => Ok(DiaSemana::Sabado),
            "SUNDAY" => Ok(DiaSemana::Domingo),
            _ => Err(ErrorDominio::DiaInvalido {
                dia: valor.to_string(),
            }),
        }
    }
}

/// El estado de un lapso, derivado de `lapse_status` de la base.
///
/// No requiere `TryFrom`: llega parseado del texto de la base; un texto que no
/// corresponda a ningún estado es un `None` (nunca debería pasar si la base
/// está sana, pero el tipo lo deja explícito en vez de asumir el `unreachable`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstadoLapso {
    Planeado,
    Activo,
    Cerrado,
}

impl EstadoLapso {
    /// Parsea el texto que devuelve `status::text` de la base.
    pub fn parse(texto: &str) -> Option<EstadoLapso> {
        match texto {
            "PLANNED" => Some(EstadoLapso::Planeado),
            "ACTIVE" => Some(EstadoLapso::Activo),
            "CLOSED" => Some(EstadoLapso::Cerrado),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            EstadoLapso::Planeado => "PLANNED",
            EstadoLapso::Activo => "ACTIVE",
            EstadoLapso::Cerrado => "CLOSED",
        }
    }
}

impl fmt::Display for EstadoLapso {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Un lapso ya validado, listo para bindear a la base (RF-1).
///
/// `practice_day` es un `DiaSemana` y `start_date`/`end_date` un `NaiveDate`:
/// es imposible construir esta struct con un día o una fecha inválidos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatosLapso {
    pub school_year: i32,
    pub lapse_number: i16,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub practice_day: DiaSemana,
}

/// La operación de ciclo de vida que el servicio quiere aplicar a un lapso.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operacion {
    Activar,
    Cerrar,
    Editar,
    Eliminar,
}

impl Operacion {
    /// La forma verbal que entra en el mensaje "no se puede {operacion}: ...".
    fn en_sujeto(&self) -> &'static str {
        match self {
            Operacion::Activar => "activar",
            Operacion::Cerrar => "cerrar",
            Operacion::Editar => "editar",
            Operacion::Eliminar => "eliminar",
        }
    }
}

/// Punto único de validación del alta y la edición (RF-2 y RF-8).
///
/// Los cinco campos llegan como texto — incluso el año y el número — para que
/// todos los mensajes salgan del dominio, en español, y no de un error de
/// `serde` o de un parseo repartido por el handler. Las fechas se normalizan a
/// `NaiveDate` (`AAA-MM-DD`) y el día a su forma canónica mayúscula.
///
/// No compara nada contra la fecha de hoy: un lapso puede registrarse con
/// fechas pasadas (decisión de la clarificación del 2026-10-07, spec
/// Definiciones). El único chequeo temporal del dominio es el orden de las
/// fechas y el del borrado (ver `validar_borrado_permitido`).
pub fn preparar_entrada(
    anio: &str,
    numero: &str,
    inicio: &str,
    fin: &str,
    dia: &str,
) -> Result<DatosLapso, ErrorDominio> {
    let anio = parsear_anio(anio)?;
    let numero = parsear_numero(numero)?;
    let start_date = parsear_fecha(inicio, "fecha de inicio")?;
    let end_date = parsear_fecha(fin, "fecha de fin")?;
    let practice_day = DiaSemana::try_from(dia)?;

    if end_date < start_date {
        return Err(ErrorDominio::FechasInvertidas);
    }

    Ok(DatosLapso {
        school_year: anio,
        lapse_number: numero,
        start_date,
        end_date,
        practice_day,
    })
}

fn parsear_anio(valor: &str) -> Result<i32, ErrorDominio> {
    let anio: i32 = valor.trim().parse().map_err(|_| ErrorDominio::AnioFueraDeRango)?;
    if !(ANIO_MINIMO..=ANIO_MAXIMO).contains(&anio) {
        return Err(ErrorDominio::AnioFueraDeRango);
    }
    Ok(anio)
}

fn parsear_numero(valor: &str) -> Result<i16, ErrorDominio> {
    let numero: i16 = valor.trim().parse().map_err(|_| ErrorDominio::NumeroFueraDeRango)?;
    if !(NUMERO_MINIMO..=NUMERO_MAXIMO).contains(&numero) {
        return Err(ErrorDominio::NumeroFueraDeRango);
    }
    Ok(numero)
}

fn parsear_fecha(valor: &str, campo: &'static str) -> Result<NaiveDate, ErrorDominio> {
    NaiveDate::parse_from_str(valor.trim(), "%Y-%m-%d").map_err(|_| ErrorDominio::FechaInvalida {
        campo,
        valor: valor.trim().to_string(),
    })
}

fn lista_dias() -> String {
    DiaSemana::TODOS
        .iter()
        .map(|dia| dia.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Avala que `estado` permite la operación (RF-8, RF-9, RF-12, RF-14, RF-15,
/// RF-16, RF-17, RF-18).
///
/// El ciclo fijado en la clarificación es `PLANNED → ACTIVE → CLOSED`, sin
/// retrocesos: activar solo desde `PLANNED`, cerrar solo desde `ACTIVE`, y
/// editar/eliminar nunca sobre un `CLOSED`. El mensaje dice el estado
/// esperado y el actual (RNF-4).
pub fn validar_transicion(
    estado: &EstadoLapso,
    operacion: Operacion,
) -> Result<(), ErrorDominio> {
    let permitido = match operacion {
        Operacion::Activar => estado == &EstadoLapso::Planeado,
        Operacion::Cerrar => estado == &EstadoLapso::Activo,
        Operacion::Editar | Operacion::Eliminar => estado != &EstadoLapso::Cerrado,
    };

    if permitido {
        return Ok(());
    }

    let esperado = match operacion {
        Operacion::Activar => "PLANNED (PLANEADO)",
        Operacion::Cerrar => "ACTIVE (ACTIVO)",
        Operacion::Editar | Operacion::Eliminar => "PLANNED o ACTIVE",
    };

    Err(ErrorDominio::EstadoIncorrecto {
        operacion: operacion.en_sujeto(),
        esperado,
        actual: estado.as_str().to_string(),
    })
}

/// La parte temporal de la excepción de borrado (RF-18, spec Definiciones):
/// un lapso ya terminado — `end_date` anterior a hoy — es historial, aunque no
/// tenga eventos. Replica la condición `end_date < CURRENT_DATE` del guardarraíl
/// `lapses_delete_guard` para dar el mensaje en 409 antes de que la base lance
/// el `23514` de su trigger (que la API no traduciría).
///
/// `hoy` se inyecta para que la función siga siendo pura y testeable sin fechas
/// atadas al reloj del equipo.
pub fn validar_borrado_permitido(
    end_date: NaiveDate,
    hoy: NaiveDate,
) -> Result<(), ErrorDominio> {
    if end_date < hoy {
        return Err(ErrorDominio::LapsoTerminado { fecha: end_date });
    }
    Ok(())
}

// Los tests de unidad pueden usar `unwrap()`: el `deny` de `lib.rs` protege
// el código de producción, y en un assert un panic es el fallo esperado.
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    type Resultado = Result<(), ErrorDominio>;

    // --- Normalización de la entrada (RF-1, RF-2) ---

    #[test]
    fn el_dia_de_practica_se_normaliza_sin_importar_forma() -> Resultado {
        let datos = preparar_entrada("2026", "1", "2026-09-14", "2026-12-18", " thursday ")?;

        assert_eq!(datos.practice_day.as_str(), "THURSDAY");
        Ok(())
    }

    #[test]
    fn dia_solo_en_mayusculas_se_acepta() -> Resultado {
        assert_eq!(DiaSemana::try_from("MONDAY")?, DiaSemana::Lunes);
        assert_eq!(DiaSemana::try_from("sunday")?, DiaSemana::Domingo);
        Ok(())
    }

    #[test]
    fn un_dia_invalido_se_rechaza() {
        // ni el inglés localizado («LUNES») ni el español («jueves») valen:
        // el catálogo es el enum `week_day` de la base, en inglés.
        assert!(matches!(
            preparar_entrada("2026", "1", "2026-09-14", "2026-12-18", "LUNES"),
            Err(ErrorDominio::DiaInvalido { .. })
        ));
        assert!(matches!(
            preparar_entrada("2026", "1", "2026-09-14", "2026-12-18", "jueves"),
            Err(ErrorDominio::DiaInvalido { .. })
        ));
        assert!(matches!(
            preparar_entrada("2026", "1", "2026-09-14", "2026-12-18", ""),
            Err(ErrorDominio::DiaInvalido { .. })
        ));
    }

    #[test]
    fn el_error_del_dia_enumera_los_siete_admitidos() {
        let Err(error) = preparar_entrada("2026", "1", "2026-09-14", "2026-12-18", "LUNES") else {
            panic!("un día inválido debe rechazarse");
        };
        let mensaje = error.to_string();

        for dia in ["MONDAY", "TUESDAY", "WEDNESDAY", "THURSDAY", "FRIDAY", "SATURDAY", "SUNDAY"]
        {
            assert!(
                mensaje.contains(dia),
                "el mensaje debe enumerar «{dia}», y fue: {mensaje}"
            );
        }
    }

    #[test]
    fn un_anio_fuera_de_rango_se_rechaza() {
        for anio in ["1999", "2101", "", "dos mil veintiseis"] {
            assert!(
                matches!(
                    preparar_entrada(anio, "1", "2026-09-14", "2026-12-18", "THURSDAY"),
                    Err(ErrorDominio::AnioFueraDeRango)
                ),
                "el año «{anio}» debe rechazarse"
            );
        }
    }

    #[test]
    fn los_bordes_del_rango_de_anio_se_aceptan() -> Resultado {
        assert_eq!(entrada_valida_con_anio(2000)?.school_year, 2000);
        assert_eq!(entrada_valida_con_anio(2100)?.school_year, 2100);
        Ok(())
    }

    #[test]
    fn un_numero_fuera_de_rango_se_rechaza() {
        for numero in ["0", "4", "x", ""] {
            assert!(
                matches!(
                    preparar_entrada("2026", numero, "2026-09-14", "2026-12-18", "THURSDAY"),
                    Err(ErrorDominio::NumeroFueraDeRango)
                ),
                "el número «{numero}» debe rechazarse"
            );
        }
    }

    #[test]
    fn los_bordes_del_numero_se_aceptan() -> Resultado {
        let datos = preparar_entrada("2026", "1", "2026-09-14", "2026-12-18", "THURSDAY")?;
        assert_eq!(datos.lapse_number, 1);

        let datos = preparar_entrada("2026", "3", "2026-09-14", "2026-12-18", "THURSDAY")?;
        assert_eq!(datos.lapse_number, 3);
        Ok(())
    }

    #[test]
    fn fechas_invertidas_se_rechazan() {
        assert!(matches!(
            preparar_entrada("2026", "1", "2026-12-18", "2026-09-14", "THURSDAY"),
            Err(ErrorDominio::FechasInvertidas)
        ));
    }

    #[test]
    fn fechas_iguales_son_validas() -> Resultado {
        let datos = preparar_entrada("2026", "3", "2026-12-18", "2026-12-18", "FRIDAY")?;

        assert_eq!(datos.start_date, datos.end_date);
        Ok(())
    }

    #[test]
    fn una_fecha_mal_formada_se_rechaza() {
        // «2026-9-14» se excluye a propósito: chrono la parsea y Postgres la
        // acepta igual que «2026-09-14» (el separador ISO admite el mes sin
        // cero), así que no es un input inválido.
        for mala in ["14/09/2026", "2026-09", "hoy", ""] {
            assert!(
                matches!(
                    preparar_entrada("2026", "1", mala, "2026-12-18", "THURSDAY"),
                    Err(ErrorDominio::FechaInvalida {
                        campo: "fecha de inicio",
                        ..
                    })
                ),
                "la fecha «{mala}» debe rechazarse como inicio"
            );
            assert!(
                matches!(
                    preparar_entrada("2026", "1", "2026-09-14", mala, "THURSDAY"),
                    Err(ErrorDominio::FechaInvalida {
                        campo: "fecha de fin",
                        ..
                    })
                ),
                "la fecha «{mala}» debe rechazarse como fin"
            );
        }
    }

    #[test]
    fn la_entrada_valida_devuelve_los_datos_armados() -> Resultado {
        let datos = preparar_entrada(" 2026 ", " 1 ", "2026-09-14", "2026-12-18", " THURSDAY ")?;

        assert_eq!(datos.school_year, 2026);
        assert_eq!(datos.lapse_number, 1);
        assert_eq!(datos.start_date, NaiveDate::from_ymd_opt(2026, 9, 14).unwrap());
        assert_eq!(datos.end_date, NaiveDate::from_ymd_opt(2026, 12, 18).unwrap());
        assert_eq!(datos.practice_day.as_str(), "THURSDAY");
        Ok(())
    }

    // --- Sin comparaciones contra "hoy" (decisión de la clarificación) ---

    #[test]
    fn la_entrada_acepta_fechas_pasadas() -> Resultado {
        // El primer término debe poder registrarse con su primera clase ya
        // ocurrida: el dominio no compara con la fecha del equipo.
        let datos = preparar_entrada("2026", "1", "2026-10-08", "2026-12-18", "THURSDAY")?;

        assert_eq!(datos.start_date, NaiveDate::from_ymd_opt(2026, 10, 8).unwrap());
        Ok(())
    }

    // --- Transiciones de estado (RF-8, RF-9, RF-12, RF-14, RF-15, RF-16) ---

    #[test]
    fn activar_se_permite_solo_desde_planeado() {
        assert!(validar_transicion(&EstadoLapso::Planeado, Operacion::Activar).is_ok());

        assert!(matches!(
            validar_transicion(&EstadoLapso::Activo, Operacion::Activar),
            Err(ErrorDominio::EstadoIncorrecto { .. })
        ));
        assert!(matches!(
            validar_transicion(&EstadoLapso::Cerrado, Operacion::Activar),
            Err(ErrorDominio::EstadoIncorrecto { .. })
        ));
    }

    #[test]
    fn cerrar_se_permite_solo_desde_activo() {
        assert!(validar_transicion(&EstadoLapso::Activo, Operacion::Cerrar).is_ok());

        assert!(matches!(
            validar_transicion(&EstadoLapso::Planeado, Operacion::Cerrar),
            Err(ErrorDominio::EstadoIncorrecto { .. })
        ));
        assert!(matches!(
            validar_transicion(&EstadoLapso::Cerrado, Operacion::Cerrar),
            Err(ErrorDominio::EstadoIncorrecto { .. })
        ));
    }

    #[test]
    fn editar_se_permite_mientras_no_este_cerrado() {
        assert!(validar_transicion(&EstadoLapso::Planeado, Operacion::Editar).is_ok());
        assert!(validar_transicion(&EstadoLapso::Activo, Operacion::Editar).is_ok());

        assert!(matches!(
            validar_transicion(&EstadoLapso::Cerrado, Operacion::Editar),
            Err(ErrorDominio::EstadoIncorrecto { .. })
        ));
    }

    #[test]
    fn eliminar_se_permite_mientras_no_este_cerrado() {
        assert!(validar_transicion(&EstadoLapso::Planeado, Operacion::Eliminar).is_ok());
        assert!(validar_transicion(&EstadoLapso::Activo, Operacion::Eliminar).is_ok());

        assert!(matches!(
            validar_transicion(&EstadoLapso::Cerrado, Operacion::Eliminar),
            Err(ErrorDominio::EstadoIncorrecto { .. })
        ));
    }

    #[test]
    fn el_error_de_estado_dice_que_espera_y_que_hay() {
        let Err(error) = validar_transicion(&EstadoLapso::Activo, Operacion::Activar) else {
            panic!("activar un activo debe rechazarse");
        };
        let mensaje = error.to_string();

        assert!(mensaje.contains("activar"), "mensaje: {mensaje}");
        assert!(mensaje.contains("PLANNED"), "mensaje: {mensaje}");
        assert!(mensaje.contains("ACTIVE"), "mensaje: {mensaje}");
    }

    #[test]
    fn el_estado_se_parsea_y_se_muestra() {
        assert_eq!(EstadoLapso::parse("PLANNED"), Some(EstadoLapso::Planeado));
        assert_eq!(EstadoLapso::parse("ACTIVE"), Some(EstadoLapso::Activo));
        assert_eq!(EstadoLapso::parse("CLOSED"), Some(EstadoLapso::Cerrado));
        assert_eq!(EstadoLapso::parse("cerrado"), None);
        assert_eq!(EstadoLapso::parse(""), None);
        assert_eq!(EstadoLapso::Planeado.as_str(), "PLANNED");
    }

    // --- Excepción de borrado (RF-18) ---

    #[test]
    fn un_lapso_terminado_no_se_puede_eliminar() {
        let hoy = NaiveDate::from_ymd_opt(2026, 12, 20).unwrap();
        let ayer = NaiveDate::from_ymd_opt(2026, 12, 19).unwrap();

        assert!(matches!(
            validar_borrado_permitido(ayer, hoy),
            Err(ErrorDominio::LapsoTerminado { fecha }) if fecha == ayer
        ));
    }

    #[test]
    fn un_lapso_con_fin_hoy_o_futuro_se_puede_eliminar() -> Resultado {
        let hoy = NaiveDate::from_ymd_opt(2026, 12, 20).unwrap();

        assert!(validar_borrado_permitido(hoy, hoy).is_ok());
        let manana = NaiveDate::from_ymd_opt(2026, 12, 21).unwrap();
        assert!(validar_borrado_permitido(manana, hoy).is_ok());
        Ok(())
    }

    // --- Mensajes en español que nombran el dato (RNF-4) ---

    #[test]
    fn los_mensajes_dicen_que_campo_fallo() {
        let anio = ErrorDominio::AnioFueraDeRango.to_string();
        let numero = ErrorDominio::NumeroFueraDeRango.to_string();
        let fecha = ErrorDominio::FechaInvalida {
            campo: "fecha de inicio",
            valor: "x".into(),
        }
        .to_string();
        let invertidas = ErrorDominio::FechasInvertidas.to_string();

        assert!(anio.contains("año escolar"), "mensaje: {anio}");
        assert!(numero.contains("número de lapso"), "mensaje: {numero}");
        assert!(fecha.contains("fecha de inicio"), "mensaje: {fecha}");
        assert!(invertidas.contains("fecha de fin"), "mensaje: {invertidas}");
    }

    // --- El catálogo de días vive en un solo lugar ---

    #[test]
    fn el_catalogo_de_dias_tiene_los_siete_en_orden() {
        let lista: Vec<&str> = DiaSemana::TODOS.iter().map(|d| d.as_str()).collect();

        assert_eq!(
            lista,
            vec![
                "MONDAY", "TUESDAY", "WEDNESDAY", "THURSDAY", "FRIDAY", "SATURDAY", "SUNDAY"
            ]
        );
    }

    #[test]
    fn todo_dia_del_catalogo_se_puede_parsear_a_si_mismo() -> Resultado {
        for dia in DiaSemana::TODOS {
            assert_eq!(DiaSemana::try_from(dia.as_str())?, dia);
        }
        Ok(())
    }

    // --- Helpers auxiliares de los tests ---

    fn entrada_valida_con_anio(anio: i32) -> Result<DatosLapso, ErrorDominio> {
        preparar_entrada(
            &anio.to_string(),
            "1",
            "2026-09-14",
            "2026-12-18",
            "THURSDAY",
        )
    }
}