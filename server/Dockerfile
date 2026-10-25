# Build from the repository root: docker build -f server/Dockerfile .
FROM rust:1-slim AS build
WORKDIR /build
COPY server/Cargo.toml server/Cargo.toml
COPY server/src server/src
COPY src/codwawzpc/coop/proto/Cargo.toml src/codwawzpc/coop/proto/Cargo.toml
COPY src/codwawzpc/coop/proto/src src/codwawzpc/coop/proto/src
# Isolate these two std-only crates from the game's native build dependencies.
RUN printf '[workspace]\nresolver = "2"\nmembers = ["server", "src/codwawzpc/coop/proto"]\n' > Cargo.toml \
    && cargo generate-lockfile --offline \
    && cargo build --locked --offline --release -p codwawzpc-server

FROM debian:stable-slim
COPY --from=build /build/target/release/codwawzpc-server /usr/local/bin/codwawzpc-server
USER 65534:65534
EXPOSE 27960/tcp
ENTRYPOINT ["codwawzpc-server"]
