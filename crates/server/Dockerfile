# Build from the repository root: docker build -f crates/server/Dockerfile .
# With the web client baked in (ghcr.io/markbang/nsh-mail): add
#   --target web --build-context site=<`nsh package --target web` site with app/web/sw.js next to index.html>
FROM rust:1-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates crates
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p nsh-mail-server \
    && mkdir -p /out/data \
    && cp target/release/nsh-mail-server /out/

FROM gcr.io/distroless/cc-debian12:nonroot AS server
COPY --from=build /out/nsh-mail-server /usr/local/bin/nsh-mail-server
COPY --from=build --chown=65532:65532 /out/data /data
ENV NSHMAIL_DATA_DIR=/data NSHMAIL_BIND=0.0.0.0:8080
VOLUME /data
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s CMD ["/usr/local/bin/nsh-mail-server", "healthcheck"]
ENTRYPOINT ["/usr/local/bin/nsh-mail-server"]

FROM server AS web
COPY --from=site . /web
ENV NSHMAIL_WEB_DIR=/web

FROM server
