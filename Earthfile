VERSION 0.6
FROM rust:latest
WORKDIR /tokei_rs

install-chef:
    ARG repo
    RUN cargo install cargo-chef
    SAVE IMAGE --push $repo/cache/install-chef

prepare:
    ARG repo
    FROM +install-chef
    COPY --dir . .
    RUN cargo chef prepare --recipe-path recipe.json
    SAVE ARTIFACT recipe.json
    SAVE IMAGE --push $repo/cache/prepare

build-deps:
    ARG repo
    FROM +install-chef
    COPY +prepare/recipe.json ./

    # build dependencies as separate layer to be cached
    RUN cargo chef cook --release --recipe-path recipe.json

    SAVE IMAGE --push $repo/cache/deps

build:
    ARG repo
    FROM +build-deps

    # copy project
    COPY --dir . .

    # build, re-using the build artifacts from cargo chef
    RUN cargo build --release

    SAVE ARTIFACT target/release/tokei_rs tokei_rs
    SAVE IMAGE --push $repo/cache/build

docker:
    ARG repo
    FROM ubuntu:jammy
    EXPOSE 8000
    COPY +build/tokei_rs tokei_rs
    ENTRYPOINT ["./tokei_rs"]
    SAVE IMAGE --push $repo

compose:
    ARG repo
    FROM earthly/dind:alpine
    WORKDIR /test
    COPY compose.yml ./
    WITH DOCKER \
            --compose compose.yml \
            --load $repo:latest(+docker)
        RUN docker compose down && docker compose up --remove-orphans
    END
