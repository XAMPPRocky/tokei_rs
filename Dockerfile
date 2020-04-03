FROM rust:latest
WORKDIR /app
RUN rustup default nightly
ADD . /app
RUN cargo build --release
