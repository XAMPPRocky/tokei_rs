CREATE TABLE repo (
    hash     VARCHAR PRIMARY KEY,
    lines    BIGINT,
    files    BIGINT,
    code    BIGINT,
    blanks    BIGINT,
    comments    BIGINT
);

CREATE USER tokei_rs;
GRANT ALL PRIVILEGES ON TABLE repo TO tokei_rs;
