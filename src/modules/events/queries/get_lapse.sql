SELECT id, school_year, lapse_number, start_date, end_date, practice_day::text AS practice_day, status::text AS status
FROM lapses
WHERE id = $1
