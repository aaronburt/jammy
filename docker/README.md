# Jammy in Docker

One small container (a Rust Rocket server, 18 MB image) that serves Jammy and
passes `/v1/*` through to Hermes. The page and the API share one address, so Hermes needs no CORS setup,
and replies still stream token by token.

```
browser ──► jammy container :8080 ──► /            Jammy page
                                  └─► /v1/...      Hermes API server (:8642)
```

## Folder layout

```
jammy/
├── jammy-chat.html          the page (the image copies it in at build time)
└── docker/
    ├── Dockerfile           multi-stage: Rust build -> alpine runtime
    ├── Dockerfile.dockerignore
    ├── server/              the Rocket server (Cargo.toml, Cargo.lock, src/main.rs)
    ├── docker-compose.yml
    ├── docker-compose.build.yml
    ├── .env.example
    └── README.md
```

## Run it

```sh
cd jammy/docker
cp .env.example .env        # then edit .env
docker compose up -d
```

This pulls `ghcr.io/aaronburt/jammy:latest`. To build from source instead:

```sh
docker compose -f docker-compose.yml -f docker-compose.build.yml up -d --build
```

To update the published image: `docker compose pull && docker compose up -d`.

Open http://localhost:8080.

All Hermes settings come from `.env`, never from the page:

| Variable | What it is |
|---|---|
| `HERMES_URL` | Hermes's API server as the container sees it, e.g. `http://192.168.1.50:8642`. Use `http://host.docker.internal:8642` only if Hermes runs on the same machine as Docker. |
| `HERMES_API_KEY` | The API server key. The server adds `Authorization: Bearer …` itself, so it never reaches the browser. |
| `HERMES_MODEL` | Optional. Forces the model name in chat requests. |

The endpoint and key fields are hidden in Jammy's Settings in this setup.
After editing `.env`, run `docker compose up -d` to apply it.

After changing `jammy-chat.html`, rebuild with the two-file command above.

Without compose:

```sh
cd jammy
docker build -t jammy -f docker/Dockerfile .
docker run -d --name jammy -p 127.0.0.1:8080:80 \
  --env-file docker/.env jammy
```

## Hermes must be reachable from the container

Hermes's API server often listens on `127.0.0.1` only, which the container
can't reach from another machine. Either make it listen on the LXC's network
address (check Hermes's API server host setting), or run the Jammy container
inside the same LXC/VM and use `HERMES_URL=http://host.docker.internal:8642`.

To check: `docker exec jammy wget -qO- --header "Authorization: Bearer KEY" $HERMES_URL/v1/models`
or use **Test connection** in Jammy's Settings.

## Security

Hermes runs tools, including a terminal, on its machine. Because the server holds
the key, anyone who can open the Jammy page can drive Hermes without a key. The
compose file therefore binds to `127.0.0.1` only. If you open it to your
network or put it behind a domain, add a login in front (your reverse proxy's
auth, Cloudflare Access, Tailscale, etc.), or leave `HERMES_API_KEY`
empty and let your reverse proxy send the Authorization header.

## What the server does

- `GET /` serves the page, patched when the server starts: it always calls
  this server's own `/v1/chat/completions` (ignoring any saved endpoint), and
  the endpoint and key fields are hidden. It refuses to start if the page no
  longer contains the text it patches, so a future page edit can't silently
  break it.
- `/v1/*` (GET and POST) is forwarded to `HERMES_URL`. The response is passed
  through as it arrives, so streaming works, and closing the tab or pressing
  Stop closes the connection to Hermes too. Bodies are capped at 20 MB.
- `GET /healthz` for the container health check.
