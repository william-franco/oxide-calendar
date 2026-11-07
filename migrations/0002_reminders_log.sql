CREATE TABLE reminders_log (
    event_id INTEGER NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    occurrence_start TEXT NOT NULL,
    PRIMARY KEY (event_id, occurrence_start)
);
