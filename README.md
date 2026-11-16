# Jammy

A chat window for a Hermes agent, with a jam-jar personality. One small Rust (Rocket) container serves the page and proxies `/v1/*` to Hermes, so there is no CORS setup and the API key stays on the server.

- `jammy-chat.html` is the whole UI (a single file).
- `docker/` has the Dockerfile, compose file and the Rocket server. See [docker/README.md](docker/README.md).

## Run it

```sh
git clone https://github.com/aaronburt/jammy && cd jammy/docker
cp .env.example .env     # set HERMES_URL and HERMES_API_KEY
docker compose up -d
```

Open http://localhost:8080.

This pulls `ghcr.io/aaronburt/jammy:latest`. To build locally instead:

```sh
docker compose -f docker-compose.yml -f docker-compose.build.yml up -d --build
```

To update: `docker compose pull && docker compose up -d`.

The first time, GitHub keeps a new package private: on github.com open your profile > Packages > jammy > Package settings and set visibility to Public (or `docker login ghcr.io` first).

## Prebuilt image

Every push to `main` builds the image in GitHub Actions and publishes it to
`ghcr.io/aaronburt/jammy` (`latest`, plus a tag per commit and per `v*` release).
To use it instead of building locally:

```sh
docker run -d -p 127.0.0.1:8080:8080 --env-file .env ghcr.io/aaronburt/jammy:latest
```

Never commit `.env`; it holds your Hermes key.
