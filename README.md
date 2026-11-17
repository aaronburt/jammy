# Jammy

A chat window for a Hermes agent, with a jam-jar personality. One small Rust (Rocket) container serves the page and proxies it to Hermes, so there is no CORS setup and the API key stays on the server.

- `web/index.html` is the whole UI (a single file).
- `server/` is the Rocket server that serves it and proxies `/v1/*` (chat) and `/api/sessions` (History) to Hermes.
- **History** lists Jammy's chats together with the sessions in Hermes, including ones from the terminal or messaging apps. Chats Jammy starts are tagged with a Hermes session id (`jammy-…`), so Hermes keeps them and you can tell which began in Jammy. It needs a Hermes with the sessions API; the two switches are under Settings → History.
- **@ in the message box** attaches a past chat (Jammy's own or a Hermes session) or an installed skill to the message. A chat goes in front of the message as a transcript (capped at about 12,000 characters); a skill becomes an instruction to load it first, since Hermes's own `@file:` and `/skill` shortcuts are not expanded through the API. The skills list comes from `GET /v1/skills`.
- `Dockerfile` and the compose files run it. See [docs/docker.md](docs/docker.md).

## Run it

```sh
git clone https://github.com/aaronburt/jammy && cd jammy
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
