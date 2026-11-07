CREATE TABLE categories (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    accent INTEGER NOT NULL DEFAULT 0,
    visible INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    uid TEXT NOT NULL UNIQUE,
    category_id INTEGER NOT NULL REFERENCES categories(id),
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    location TEXT NOT NULL DEFAULT '',
    all_day INTEGER NOT NULL DEFAULT 0,
    start_local TEXT NOT NULL,
    end_local TEXT NOT NULL,
    rrule TEXT,
    reminder_minutes INTEGER,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE event_exceptions (
    event_id INTEGER NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    original_start TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('cancelled')),
    PRIMARY KEY (event_id, original_start)
);

INSERT INTO categories (name, accent) VALUES ('Trabalho', 0), ('Pessoal', 2), ('Saúde', 4);
