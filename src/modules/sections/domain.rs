//! Normalización y validación de una sección.
//!
//! Funciones puras: sin `sqlx`, sin `async`, sin base de datos. Es el motivo
//! por el que `sections` subió de "CRUD directo" a hexagonal parcial (plan
//! §2): el grado es un catálogo cerrado y el turno tiene forma canónica, así
//! que hay reglas que valen la pena proteger del SQL.
//!
//! `preparar_entrada` es el **único** punto de validación: el alta y la edición
//! pasan por la misma función (RF-14 lo exige literalmente), y por eso no
//! pueden divergir.

use thiserror::Error;

/// Máximo de `sections.grade_level` (`varchar(30)`).
pub const MAX_GRADO: usize = 30;

/// Máximo de `sections.section` (`varchar(10)`).
pub const MAX_SECCION: usize = 10;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ErrorDominio {
    #[error("El campo «{campo}» es obligatorio")]
    CampoVacio { campo: &'static str },

    #[error("El campo «{campo}» no puede superar {max} caracteres")]
    LargoExcedido { campo: &'static str, max: usize },

    // RF-7 y RF-16 obligan a enumerar los admitidos. La lista sale del enum,
    // no de un literal: una copia escrita a mano puede desincronizarse sin que
    // nada falle (plan §8.1).
    #[error("El grado no es válido. Grados admitidos: {}", lista_grados())]
    GradoDesconocido,

    #[error("El turno no es válido. Turnos admitidos: {}", lista_turnos())]
    TurnoDesconocido,
}

/// El catálogo cerrado de grados (spec 002, Definiciones). No hay otros grados
/// y no se van a agregar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grado {
    Primero,
    Segundo,
    Tercero,
    Cuarto,
    Quinto,
}

impl Grado {
    /// En orden canónico. De acá salen la validación, el texto del error y el
    /// array del `ORDER BY` de `list_active.sql`: una sola fuente de verdad.
    pub const TODOS: [Grado; 5] = [
        Grado::Primero,
        Grado::Segundo,
        Grado::Tercero,
        Grado::Cuarto,
        Grado::Quinto,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Grado::Primero => "1ro",
            Grado::Segundo => "2do",
            Grado::Tercero => "3ro",
            Grado::Cuarto => "4to",
            Grado::Quinto => "5to",
        }
    }
}

impl<'a> TryFrom<&'a str> for Grado {
    type Error = ErrorDominio;

    /// Normaliza antes de comparar. Sin esto, `"1RO"` se rechazaría por
    /// mayúscula con un error que no explica que el problema era la forma, no
    /// el valor (RF-2 junto con RF-7).
    fn try_from(valor: &'a str) -> Result<Self, Self::Error> {
        match normalizar_texto(valor).to_lowercase().as_str() {
            "1ro" => Ok(Grado::Primero),
            "2do" => Ok(Grado::Segundo),
            "3ro" => Ok(Grado::Tercero),
            "4to" => Ok(Grado::Cuarto),
            "5to" => Ok(Grado::Quinto),
            _ => Err(ErrorDominio::GradoDesconocido),
        }
    }
}

/// Los dos turnos admitidos, guardados en mayúsculas (RF-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Turno {
    Matutino,
    Vespertino,
}

impl Turno {
    pub const TODOS: [Turno; 2] = [Turno::Matutino, Turno::Vespertino];

    pub fn as_str(&self) -> &'static str {
        match self {
            Turno::Matutino => "MATUTINO",
            Turno::Vespertino => "VESPERTINO",
        }
    }
}

impl<'a> TryFrom<&'a str> for Turno {
    type Error = ErrorDominio;

    fn try_from(valor: &'a str) -> Result<Self, Self::Error> {
        match normalizar_texto(valor).to_uppercase().as_str() {
            "MATUTINO" => Ok(Turno::Matutino),
            "VESPERTINO" => Ok(Turno::Vespertino),
            _ => Err(ErrorDominio::TurnoDesconocido),
        }
    }
}

/// Una sección ya normalizada y validada, lista para bindear.
///
/// Los campos de grado y turno son enums, no `String`: es imposible construir
/// esta struct con un grado fuera del catálogo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatosSeccion {
    pub grado: Grado,
    pub seccion: String,
    pub turno: Turno,
}

fn lista_grados() -> String {
    Grado::TODOS
        .iter()
        .map(|grado| grado.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

fn lista_turnos() -> String {
    Turno::TODOS
        .iter()
        .map(|turno| turno.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn normalizar_texto(entrada: &str) -> String {
    entrada.trim().to_string()
}

pub fn validar_no_vacio(valor: &str, campo: &'static str) -> Result<(), ErrorDominio> {
    if valor.is_empty() {
        return Err(ErrorDominio::CampoVacio { campo });
    }
    Ok(())
}

pub fn validar_largo(valor: &str, max: usize, campo: &'static str) -> Result<(), ErrorDominio> {
    // En caracteres, no en bytes: `varchar(10)` de Postgres cuenta
    // caracteres, así que medir en bytes rechazaría antes de tiempo datos que
    // la base sí acepta — y el error sería mío, no del límite real.
    if valor.chars().count() > max {
        return Err(ErrorDominio::LargoExcedido { campo, max });
    }
    Ok(())
}

/// Punto único de validación del alta y la edición.
///
/// El orden es recortar → no vacío → largo → catálogo, y no es arbitrario:
/// normalizar **antes** de validar contra el catálogo es lo que hace que
/// `"1RO"` se acepte y se guarde como `1ro` (RF-2 con RF-7), en vez de
/// rechazarse por la mayúscula.
pub fn preparar_entrada(
    grade_level: &str,
    seccion: &str,
    shift: &str,
) -> Result<DatosSeccion, ErrorDominio> {
    let grado = normalizar_texto(grade_level).to_lowercase();
    let seccion = normalizar_texto(seccion).to_lowercase();
    let turno = normalizar_texto(shift).to_uppercase();

    validar_no_vacio(&grado, "grado")?;
    validar_no_vacio(&seccion, "subdivisión")?;
    validar_no_vacio(&turno, "turno")?;

    validar_largo(&grado, MAX_GRADO, "grado")?;
    validar_largo(&seccion, MAX_SECCION, "subdivisión")?;

    Ok(DatosSeccion {
        grado: Grado::try_from(grado.as_str())?,
        seccion,
        turno: Turno::try_from(turno.as_str())?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Normalización del alta y la edición (RF-2, RF-3, RF-4, RF-14) ---

    #[test]
    fn grado_se_guarda_en_minusculas_y_recortado() -> Result<(), ErrorDominio> {
        let datos = preparar_entrada(" 1RO ", "A", "MATUTINO")?;

        assert_eq!(datos.grado.as_str(), "1ro");
        assert_eq!(datos.seccion, "a");
        Ok(())
    }

    #[test]
    fn turno_se_guarda_en_mayusculas() -> Result<(), ErrorDominio> {
        let datos = preparar_entrada("1ro", "a", "matutino")?;

        assert_eq!(datos.turno.as_str(), "MATUTINO");
        Ok(())
    }

    #[test]
    fn grado_ya_normalizado_no_cambia() -> Result<(), ErrorDominio> {
        let datos = preparar_entrada("  1ro  ", "  a  ", "Vespertino")?;

        assert_eq!(datos.grado.as_str(), "1ro");
        assert_eq!(datos.seccion, "a");
        assert_eq!(datos.turno.as_str(), "VESPERTINO");
        Ok(())
    }

    #[test]
    fn el_grado_normaliza_dentro_del_propio_enum() -> Result<(), ErrorDominio> {
        assert_eq!(Grado::try_from("2DO")?, Grado::Segundo);
        assert_eq!(Grado::try_from(" 5to ")?.as_str(), "5to");
        Ok(())
    }

    // --- Datos obligatorios (RF-5, RF-6, RF-15) ---

    #[test]
    fn grado_vacio_se_rechaza() {
        assert!(matches!(
            preparar_entrada("", "a", "MATUTINO"),
            Err(ErrorDominio::CampoVacio { campo: "grado" })
        ));
    }

    #[test]
    fn grado_solo_con_espacios_se_rechaza() {
        assert!(matches!(
            preparar_entrada("   ", "a", "MATUTINO"),
            Err(ErrorDominio::CampoVacio { campo: "grado" })
        ));
    }

    #[test]
    fn subdivision_vacia_se_rechaza() {
        assert!(matches!(
            preparar_entrada("1ro", "  ", "MATUTINO"),
            Err(ErrorDominio::CampoVacio {
                campo: "subdivisión"
            })
        ));
    }

    #[test]
    fn turno_vacio_se_rechaza() {
        assert!(matches!(
            preparar_entrada("1ro", "a", ""),
            Err(ErrorDominio::CampoVacio { campo: "turno" })
        ));
    }

    // --- Catálogo cerrado (RF-7, RF-16) ---

    #[test]
    fn grado_fuera_del_catalogo_se_rechaza() {
        assert!(matches!(
            preparar_entrada("quinto", "a", "MATUTINO"),
            Err(ErrorDominio::GradoDesconocido)
        ));
    }

    #[test]
    fn grado_sexto_se_rechaza() {
        assert!(matches!(
            preparar_entrada("6to", "a", "MATUTINO"),
            Err(ErrorDominio::GradoDesconocido)
        ));
    }

    #[test]
    fn turno_desconocido_se_rechaza_aunque_venga_en_mayusculas() {
        assert!(matches!(
            preparar_entrada("1ro", "a", "NOCTURNO"),
            Err(ErrorDominio::TurnoDesconocido)
        ));
    }

    #[test]
    fn el_error_de_grado_enumera_los_cinco_admitidos() {
        let Err(error) = preparar_entrada("quinto", "a", "MATUTINO") else {
            panic!("un grado fuera del catálogo debe rechazarse");
        };
        let mensaje = error.to_string();

        for grado in ["1ro", "2do", "3ro", "4to", "5to"] {
            assert!(
                mensaje.contains(grado),
                "el mensaje debe enumerar «{grado}», y fue: {mensaje}"
            );
        }
    }

    #[test]
    fn el_error_de_turno_enumera_los_dos_admitidos() {
        let Err(error) = preparar_entrada("1ro", "a", "NOCTURNO") else {
            panic!("un turno desconocido debe rechazarse");
        };
        let mensaje = error.to_string();

        assert!(
            mensaje.contains("MATUTINO") && mensaje.contains("VESPERTINO"),
            "el mensaje debe enumerar los dos turnos, y fue: {mensaje}"
        );
    }

    // --- Límite de longitud (caso límite "subdivisión larga") ---

    #[test]
    fn subdivision_mas_larga_que_el_maximo_se_rechaza() {
        assert!(matches!(
            preparar_entrada("1ro", "abcdefghijk", "MATUTINO"),
            Err(ErrorDominio::LargoExcedido {
                campo: "subdivisión",
                max: 10
            })
        ));
    }

    #[test]
    fn grado_mas_largo_que_el_maximo_se_rechaza() {
        let grado_largo = "x".repeat(31);

        assert!(matches!(
            preparar_entrada(&grado_largo, "a", "MATUTINO"),
            Err(ErrorDominio::LargoExcedido {
                campo: "grado",
                max: 30
            })
        ));
    }

    #[test]
    fn el_largo_se_mide_en_caracteres_no_en_bytes() -> Result<(), ErrorDominio> {
        // 10 caracteres multibyte ocupan más de 10 bytes: si se midiera en
        // bytes, `varchar(10)` de Postgres la rechazaría después.
        let datos = preparar_entrada("1ro", "áéíóúáéíóú", "MATUTINO")?;

        assert_eq!(datos.seccion, "áéíóúáéíóú");
        Ok(())
    }

    // --- El catálogo vive en un solo lugar (RF-2, RF-7) ---

    #[test]
    fn el_catalogo_de_grados_tiene_los_cinco_en_orden() {
        let lista: Vec<&str> = Grado::TODOS.iter().map(|g| g.as_str()).collect();

        assert_eq!(lista, vec!["1ro", "2do", "3ro", "4to", "5to"]);
    }

    #[test]
    fn el_catalogo_de_turnos_tiene_los_dos() {
        let lista: Vec<&str> = Turno::TODOS.iter().map(|t| t.as_str()).collect();

        assert_eq!(lista, vec!["MATUTINO", "VESPERTINO"]);
    }

    #[test]
    fn todo_grado_del_catalogo_se_puede_parsear_a_si_mismo() -> Result<(), ErrorDominio> {
        for grado in Grado::TODOS {
            assert_eq!(Grado::try_from(grado.as_str())?, grado);
        }
        Ok(())
    }

    // --- Mensajes en español que nombran el campo (RNF-4) ---

    #[test]
    fn los_mensajes_de_error_dicen_que_dato_fallo() {
        let vacio = ErrorDominio::CampoVacio {
            campo: "subdivisión",
        }
        .to_string();
        let largo = ErrorDominio::LargoExcedido {
            campo: "subdivisión",
            max: 10,
        }
        .to_string();

        assert!(vacio.contains("subdivisión"), "mensaje: {vacio}");
        assert!(largo.contains("subdivisión"), "mensaje: {largo}");
        assert!(largo.contains("10"), "mensaje: {largo}");
    }

    // --- Funciones de normalización sueltas ---

    #[test]
    fn normalizar_texto_recorta_extremos() {
        assert_eq!(normalizar_texto("  hola  "), "hola");
        assert_eq!(normalizar_texto("hola"), "hola");
        assert_eq!(normalizar_texto("   "), "");
    }
}
