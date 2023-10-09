FROM rust:1 as build-env
WORKDIR /app
COPY . /app
RUN cargo build --release

FROM debian:12-slim
ENV MODE="ws_to_tcp" BIND="" DEST=""
RUN useradd -ms /bin/bash rust && mkdir /app && chown rust:rust /app
USER rust
WORKDIR /app
COPY --from=build-env /app/target/release/ws_bridge_rs /app/ws_bridge_rs
ENTRYPOINT ["/app/ws_bridge_rs"]
