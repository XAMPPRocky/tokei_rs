DROP TABLE IF EXISTS repo;
DROP TABLE IF EXISTS stats;

CREATE TABLE repo (
    hash     VARCHAR PRIMARY KEY,
    blanks    BIGINT NOT NULL,
    code    BIGINT NOT NULL,
    comments    BIGINT NOT NULL,
    lines    BIGINT NOT NULL
);

CREATE TABLE stats (
    hash     VARCHAR NOT NULL,
    blanks    BIGINT NOT NULL,
    code    BIGINT NOT NULL,
    comments    BIGINT NOT NULL,
    lines    BIGINT NOT NULL,
    name    VARCHAR NOT NULL
);

DROP USER IF EXISTS tokei_rs;
CREATE USER tokei_rs;
GRANT ALL PRIVILEGES ON TABLE repo TO tokei_rs;
GRANT ALL PRIVILEGES ON TABLE stats TO tokei_rs;
