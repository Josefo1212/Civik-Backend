UPDATE lapses
SET practice_day = $2::text::week_day, updated_at = now()
WHERE id = $1
RETURNING id, school_year, lapse_number, start_date, end_date, practice_day::text AS practice_day, status::text AS status