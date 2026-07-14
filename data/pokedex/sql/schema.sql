CREATE TABLE poketype (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL
);

CREATE TABLE ability (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL
);

CREATE TABLE move_method (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL
);

CREATE TABLE pokemon (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    generation INTEGER NOT NULL
);

CREATE TABLE move (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    power INTEGER,
    poketype_id INTEGER NOT NULL REFERENCES poketype(id),
    damage_class TEXT NOT NULL
);

CREATE TABLE pokemon_poketype (
    pokemon_id INTEGER NOT NULL REFERENCES pokemon(id),
    poketype_id INTEGER NOT NULL REFERENCES poketype(id),
    slot INTEGER NOT NULL,
    PRIMARY KEY (pokemon_id, slot)
);

CREATE TABLE pokemon_ability (
    pokemon_id INTEGER NOT NULL REFERENCES pokemon(id),
    ability_id INTEGER NOT NULL REFERENCES ability(id),
    is_hidden BOOLEAN NOT NULL,
    slot INTEGER NOT NULL,
    PRIMARY KEY (pokemon_id, slot)
);

CREATE TABLE pokemon_move (
    pokemon_id INTEGER NOT NULL REFERENCES pokemon(id),
    move_id INTEGER NOT NULL REFERENCES move(id),
    method_id INTEGER NOT NULL REFERENCES move_method(id),
    level INTEGER NOT NULL,
    PRIMARY KEY (pokemon_id, move_id, method_id, level)
);

CREATE TABLE efficacy (
    damage_poketype_id INTEGER NOT NULL REFERENCES poketype(id),
    target_poketype_id INTEGER NOT NULL REFERENCES poketype(id),
    factor INTEGER NOT NULL,
    PRIMARY KEY (damage_poketype_id, target_poketype_id)
);
