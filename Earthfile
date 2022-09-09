VERSION 0.6
FROM rust:latest
WORKDIR /tokei_rs

install-chef:
    RUN cargo install cargo-chef
    SAVE IMAGE --push ghcr.io/xampprocky/tokei_rs/cache/install-chef

prepare:
    FROM +install-chef
    COPY --dir . .
    RUN cargo chef prepare --recipe-path recipe.json
    SAVE ARTIFACT recipe.json
    SAVE IMAGE --push ghcr.io/xampprocky/tokei_rs/cache/prepare

build-deps:
    FROM +install-chef
    COPY +prepare/recipe.json ./

    # build dependencies as separate layer to be cached
    RUN cargo chef cook --release --recipe-path recipe.json

    SAVE IMAGE --push ghcr.io/xampprocky/tokei_rs/cache/deps

build:
    FROM +build-deps

    # copy project
    COPY --dir . .

    # build, re-using the build artifacts from cargo chef
    RUN cargo build --release

    SAVE ARTIFACT target/release/tokei_rs tokei_rs
    SAVE IMAGE --push ghcr.io/xampprocky/tokei_rs/cache/build

docker:
    FROM ubuntu:jammy
    EXPOSE 8000
    COPY +build/tokei_rs tokei_rs
    ENTRYPOINT ["./tokei_rs"]
    SAVE IMAGE --push ghcr.io/xampprocky/tokei_rs

compose:
    FROM earthly/dind:alpine
    WORKDIR /test
    COPY compose.yml ./
    WITH DOCKER \
            --compose compose.yml \
            --load ghcr.io/xampprocky/tokei_rs:latest(+docker)
        RUN docker compose down && docker compose up --remove-orphans
    END
