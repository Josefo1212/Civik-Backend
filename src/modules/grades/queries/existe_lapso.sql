-- Pre-chequeo del service para RF-10: un lapso inexistente responde 404.
SELECT EXISTS (SELECT 1 FROM lapses WHERE id = $1) AS existe
