# Jammy: the chat window for Hermes, served by a small Rust (Rocket) server
# that also proxies /v1/* to Hermes. Build from the jammy/ folder:
#   docker build -t jammy -f docker/Dockerfile .

FROM rust:1-alpine AS build
RUN apk add --no-cache musl-dev
WORKDIR /src
# Cache dependencies first
COPY docker/server/Cargo.toml ./
RUN mkdir src && echo 'fn main(){}' > src/main.rs && cargo build --release && rm -rf src target/release/jammy-server*
COPY docker/server/src ./src
RUN touch src/main.rs && cargo build --release

FROM alpine:3.20
RUN adduser -D -H jammy
WORKDIR /app
COPY --from=build /src/target/release/jammy-server /app/jammy-server
COPY jammy-chat.html /app/jammy-chat.html
ENV HERMES_URL=http://host.docker.internal:8642 \
    JAMMY_PAGE=/app/jammy-chat.html \
    ROCKET_PORT=8080
# Non-root, so listen on 8080 inside the container
USER jammy
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=3s CMD wget -qO- http://127.0.0.1:8080/healthz >/dev/null || exit 1
CMD ["/app/jammy-server"]
