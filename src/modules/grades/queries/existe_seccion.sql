-- Pre-chequeo del service para RF-11: una sección inexistente responde 404.
SELECT EXISTS (SELECT 1 FROM sections WHERE id = $1) AS existe
