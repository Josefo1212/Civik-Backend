//! Normalización y validación de la entrada de un estudiante.
//!
//! Funciones puras: sin `sqlx`, sin `async`, sin base de datos. `students` se
//! mantiene como CRUD directo (plan 003 §2): acá no vive negocio multi-tabla,
//! solo la limpieza de los datos que teclea el instructor.
//!
//! `preparar_entrada` es el **único** punto de normalización: el alta y la
//! edición pasan por la misma función (RF-10 lo exige literalmente), y por eso
//! no pueden divergir.

use thiserror::Error;
use uuid::Uuid;

/// Máximo de `students.first_name` y `students.last_name` (`varchar(50)`).
pub const MAX_NOMBRE: usize = 50;

/// Máximo de `students.phone` (`varchar(30)`).
pub const MAX_TELEFONO: usize = 30;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ErrorEntrada {
    #[error("El campo «{campo}» es obligatorio")]
    CampoVacio { campo: &'static str },

    #[error("El campo «{campo}» no puede superar {max} caracteres")]
    LargoExcedido { campo: &'static str, max: usize },
}

/// Un estudiante ya normalizado y validado, listo para bindear.
///
/// Los nombres se conservan con la capitalización que tecleó el instructor
/// (decisión de clarificación); `telefono` es `Option` porque el campo es
/// opcional en la base y el `""` ya quedó como `None` acá, antes de llegar a
/// la query. No hay cédula: la spec la dejó fuera desde la clarificación.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatosEstudiante {
    pub id_seccion: Uuid,
    pub apellido: String,
    pub nombre: String,
    pub telefono: Option<String>,
}

/// Recorta los extremos y colapsa cualquier runa de blancos (espacios, tabs,
/// saltos de línea) a un solo espacio, sin tocar la capitalización.
///
/// Es distinta de `sections::normalizar_texto` (que solo recorta extremos):
/// nombres propios como `" Ana    María "` no deben viajar a la base con
/// huecos dobles (RF-6), pero tampoco hay que reescribir cómo escribe las
/// personas el instructor.
pub fn colapsar_espacios(entrada: &str) -> String {
    entrada.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn validar_no_vacio(valor: &str, campo: &'static str) -> Result<(), ErrorEntrada> {
    if valor.is_empty() {
        return Err(ErrorEntrada::CampoVacio { campo });
    }
    Ok(())
}

pub fn validar_largo(valor: &str, max: usize, campo: &'static str) -> Result<(), ErrorEntrada> {
    // En caracteres, no en bytes: `varchar(50)` de Postgres cuenta caracteres,
    // así que medir en bytes rechazaría antes de tiempo datos que la base sí
    // acepta — y el error sería mío, no del límite real.
    if valor.chars().count() > max {
        return Err(ErrorEntrada::LargoExcedido { campo, max });
    }
    Ok(())
}

/// Colapsa y valida un campo opcional: `None` o un blanco colapsado terminan
/// en `None`, y un valor presente debe caber en su límite.
fn opcional_normalizado(
    valor: Option<&str>,
    max: usize,
    campo: &'static str,
) -> Result<Option<String>, ErrorEntrada> {
    let Some(texto) = valor else {
        return Ok(None);
    };

    let colapsado = colapsar_espacios(texto);
    if colapsado.is_empty() {
        return Ok(None);
    }

    validar_largo(&colapsado, max, campo)?;
    Ok(Some(colapsado))
}

/// Punto único de validación del alta y la edición (RF-10).
///
/// El orden es colapsar → no vacío → largo → opcionales. Colapsar **antes**
/// de validar es lo que hace que `" Ana    María "` se acepte como
/// `Ana María`, y que un `"   "` sea un vacío y no un apellido de espacios
/// (RF-2).
pub fn preparar_entrada(
    id_seccion: Uuid,
    apellido: &str,
    nombre: &str,
    telefono: Option<&str>,
) -> Result<DatosEstudiante, ErrorEntrada> {
    let apellido = colapsar_espacios(apellido);
    let nombre = colapsar_espacios(nombre);

    validar_no_vacio(&apellido, "apellido")?;
    validar_no_vacio(&nombre, "nombre")?;

    validar_largo(&apellido, MAX_NOMBRE, "apellido")?;
    validar_largo(&nombre, MAX_NOMBRE, "nombre")?;

    let telefono = opcional_normalizado(telefono, MAX_TELEFONO, "teléfono")?;

    Ok(DatosEstudiante {
        id_seccion,
        apellido,
        nombre,
        telefono,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id_prueba() -> Uuid {
        Uuid::nil()
    }

    // --- Colapso de blancos y capitalización (RF-5) ---

    #[test]
    fn colapsa_extremos_y_espacios_repetidos() {
        assert_eq!(colapsar_espacios("  Ana    María  "), "Ana María");
        assert_eq!(colapsar_espacios("Ana"), "Ana");
    }

    #[test]
    fn colapsa_tabs_y_saltos_de_linea() {
        assert_eq!(colapsar_espacios("Ana\tJosé\nPérez"), "Ana José Pérez");
    }

    #[test]
    fn un_blanco_completo_colapsa_a_vacio() {
        assert_eq!(colapsar_espacios("   \t  "), "");
    }

    #[test]
    fn la_capitalizacion_se_conserva() {
        assert_eq!(colapsar_espacios("de la cruz"), "de la cruz");
        assert_eq!(colapsar_espacios("De La Cruz"), "De La Cruz");
    }

    // --- Datos obligatorios (RF-2, RF-11) ---

    #[test]
    fn apellido_vacio_se_rechaza() {
        assert!(matches!(
            preparar_entrada(id_prueba(), "", "Ana", None),
            Err(ErrorEntrada::CampoVacio { campo: "apellido" })
        ));
    }

    #[test]
    fn apellido_solo_con_espacios_se_rechaza() {
        assert!(matches!(
            preparar_entrada(id_prueba(), "   ", "Ana", None),
            Err(ErrorEntrada::CampoVacio { campo: "apellido" })
        ));
    }

    #[test]
    fn nombre_vacio_se_rechaza() {
        assert!(matches!(
            preparar_entrada(id_prueba(), "Pérez", "", None),
            Err(ErrorEntrada::CampoVacio { campo: "nombre" })
        ));
    }

    // --- El alta y la edición comparten el mismo camino (RF-10) ---

    #[test]
    fn entrada_normalizada_es_la_misma_para_cualquier_llamada() -> Result<(), ErrorEntrada> {
        let datos = preparar_entrada(id_prueba(), "  Ana    María ", "  Pérez  ", None)?;

        assert_eq!(datos.apellido, "Ana María");
        assert_eq!(datos.nombre, "Pérez");
        assert_eq!(datos.id_seccion, id_prueba());
        Ok(())
    }

    // --- Teléfono opcional ---

    #[test]
    fn telefono_ausente_queda_como_none() -> Result<(), ErrorEntrada> {
        let datos = preparar_entrada(id_prueba(), "Ana", "Pérez", None)?;

        assert_eq!(datos.telefono, None);
        Ok(())
    }

    #[test]
    fn telefono_en_blanco_queda_como_none() -> Result<(), ErrorEntrada> {
        let datos = preparar_entrada(id_prueba(), "Ana", "Pérez", Some(""))?;

        assert_eq!(datos.telefono, None);
        Ok(())
    }

    #[test]
    fn telefono_con_espacios_se_recorta() -> Result<(), ErrorEntrada> {
        let datos = preparar_entrada(id_prueba(), "Ana", "Pérez", Some(" 04121 "))?;

        assert_eq!(datos.telefono.as_deref(), Some("04121"));
        Ok(())
    }

    // --- Límite de longitud ---

    #[test]
    fn nombre_mas_largo_que_el_maximo_se_rechaza() {
        let nombre_largo = "e".repeat(51);

        assert!(matches!(
            preparar_entrada(id_prueba(), "Ana", &nombre_largo, None),
            Err(ErrorEntrada::LargoExcedido {
                campo: "nombre",
                max: 50
            })
        ));
    }

    #[test]
    fn apellido_mas_largo_que_el_maximo_se_rechaza() {
        let apellido_largo = "e".repeat(51);

        assert!(matches!(
            preparar_entrada(id_prueba(), &apellido_largo, "Ana", None),
            Err(ErrorEntrada::LargoExcedido {
                campo: "apellido",
                max: 50
            })
        ));
    }

    #[test]
    fn telefono_mas_largo_que_el_maximo_se_rechaza() {
        assert!(matches!(
            preparar_entrada(id_prueba(), "Ana", "Pérez", Some("0".repeat(31).as_str())),
            Err(ErrorEntrada::LargoExcedido {
                campo: "teléfono",
                max: 30
            })
        ));
    }

    #[test]
    fn el_largo_se_mide_en_caracteres_no_en_bytes() -> Result<(), ErrorEntrada> {
        // 50 caracteres multibyte ocupan más de 50 bytes: si se midiera en
        // bytes, `varchar(50)` de Postgres la rechazaría después.
        let nombre_multibyte = "á".repeat(50);

        let datos = preparar_entrada(id_prueba(), "Ana", &nombre_multibyte, None)?;

        assert_eq!(datos.nombre, nombre_multibyte);
        Ok(())
    }

    // --- Mensajes en español que nombran el campo (RNF-4) ---

    #[test]
    fn los_mensajes_de_error_dicen_que_dato_fallo() {
        let vacio = ErrorEntrada::CampoVacio { campo: "apellido" }.to_string();
        let largo = ErrorEntrada::LargoExcedido {
            campo: "teléfono",
            max: 30,
        }
        .to_string();

        assert!(vacio.contains("apellido"), "mensaje: {vacio}");
        assert!(largo.contains("teléfono"), "mensaje: {largo}");
        assert!(largo.contains("30"), "mensaje: {largo}");
    }
}
