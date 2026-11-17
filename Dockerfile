FROM rust:1-alpine AS build
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY server/Cargo.toml server/Cargo.lock ./
RUN mkdir src && echo 'fn main(){}' > src/main.rs && cargo build --release && rm -rf src target/release/jammy-server*
COPY server/src ./src
RUN touch src/main.rs && cargo build --release

FROM alpine:3.20
RUN adduser -D -H jammy
WORKDIR /app
COPY --from=build /src/target/release/jammy-server /app/jammy-server
COPY web/index.html /app/index.html
ENV HERMES_URL=http://host.docker.internal:8642 \
    JAMMY_PAGE=/app/index.html \
    ROCKET_PORT=8080
USER jammy
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=3s CMD wget -qO- http://127.0.0.1:8080/healthz >/dev/null || exit 1
CMD ["/app/jammy-server"]
