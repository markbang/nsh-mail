# Build from the repository root: docker build -f crates/server/Dockerfile .
FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p nsh-mail-server \
    && mkdir -p /out/data \
    && cp target/release/nsh-mail-server /out/

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /out/nsh-mail-server /usr/local/bin/nsh-mail-server
COPY --from=build --chown=65532:65532 /out/data /data
ENV NSHMAIL_DATA_DIR=/data NSHMAIL_BIND=0.0.0.0:8080
VOLUME /data
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/nsh-mail-server"]
