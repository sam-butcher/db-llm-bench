CREATE TABLE cars (
    id SERIAL PRIMARY KEY,
    brand TEXT NOT NULL,
    model TEXT NOT NULL,
    wheels INTEGER NOT NULL,
    price NUMERIC(10, 2) NOT NULL,
    registered DATE NOT NULL
);
