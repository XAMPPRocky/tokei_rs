CREATE TABLE repo (
    hash     VARCHAR PRIMARY KEY,
    lines    BIGINT NOT NULL,
    files    BIGINT NOT NULL,
    code    BIGINT NOT NULL,
    blanks    BIGINT NOT NULL,
    comments    BIGINT NOT NULL
);

CREATE TABLE stats (
    hash     VARCHAR,
    name    VARCHAR NOT NULL,
    lines    BIGINT NOT NULL,
    code    BIGINT NOT NULL,
    blanks    BIGINT NOT NULL,
    comments    BIGINT NOT NULL
);

CREATE USER tokei_rs;
GRANT ALL PRIVILEGES ON TABLE repo TO tokei_rs;
GRANT ALL PRIVILEGES ON TABLE stats TO tokei_rs;
