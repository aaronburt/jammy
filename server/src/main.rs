//! Jammy server: serves the Jammy page and reverse-proxies /v1/* (chat) and the
//! /api/sessions routes (History) to Hermes, streaming responses through without buffering.
//!
//! Config comes from the environment (.env via docker compose), never the page:
//!      HERMES_URL (default http://host.docker.internal:8642),
//!      HERMES_API_KEY (optional; the server adds the Bearer key),
//!      HERMES_MODEL (optional; overrides the model name in chat requests),
//!      JAMMY_PAGE (path to the page, default /app/index.html).

#[macro_use]
extern crate rocket;

use futures_util::TryStreamExt;
use rocket::data::{Data, ToByteUnit};
use rocket::http::{ContentType, Header, Status};
use rocket::request::{FromRequest, Outcome, Request};
use rocket::response::content::RawHtml;
use rocket::State;
use std::io;
use tokio_util::io::StreamReader;

struct Config {
    upstream: String,
    key: Option<String>,
    model: Option<String>,
    page: String,
    client: reqwest::Client,
}

/// Request headers we may forward to Hermes.
struct Fwd {
    authorization: Option<String>,
    content_type: Option<String>,
    accept: Option<String>,
    /// X-Hermes-Session-Id: the Hermes session a chat belongs to (only passed on if it looks like a plain id).
    session_id: Option<String>,
    /// The raw query string, passed through untouched (percent-encoding and all).
    query: Option<String>,
}

/// Session ids are plain tokens (Jammy's are `jammy-...`); anything else is not forwarded or routed.
fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'))
}

/// Only sessions Jammy itself created (`jammy-...`) may be renamed or deleted through this server.
fn is_jammy_id(id: &str) -> bool {
    safe_id(id) && id.starts_with("jammy-")
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for Fwd {
    type Error = ();
    async fn from_request(r: &'r Request<'_>) -> Outcome<Self, ()> {
        let h = r.headers();
        Outcome::Success(Fwd {
            authorization: h.get_one("Authorization").map(str::to_owned),
            content_type: h.get_one("Content-Type").map(str::to_owned),
            accept: h.get_one("Accept").map(str::to_owned),
            session_id: h.get_one("X-Hermes-Session-Id").filter(|v| safe_id(v)).map(str::to_owned),
            query: r.uri().query().map(|q| q.as_str().to_owned()).filter(|q| !q.is_empty()),
        })
    }
}

/// Patch the page: default endpoint -> same origin, and tell it whether the
/// server holds the key. Fails at startup if the page changed shape.
fn patch_page(html: &str) -> Result<String, String> {
    let mut s = html.to_owned();
    let swaps: [(&str, &str); 6] = [
        // Endpoint and key come from .env on the server, so the page never
        // holds them: hide those two Settings fields.
        (
            "<div class=\"field\">\n      <label for=\"sEndpoint\">",
            "<div class=\"field\" hidden>\n      <label for=\"sEndpoint\">",
        ),
        (
            "<div class=\"field\">\n      <label for=\"sKey\">",
            "<div class=\"field\" hidden>\n      <label for=\"sKey\">",
        ),
        (
            "\"http://127.0.0.1:8642/v1/chat/completions\"",
            "location.origin + \"/v1/chat/completions\"",
        ),
        // Always use this server, whatever an old browser save says.
        (
            "}, store.get(\"settings\", {}));",
            "}, store.get(\"settings\", {}), { endpoint: location.origin + \"/v1/chat/completions\", key: \"\" });",
        ),
        (
            "if (!settings.key) throw",
            "if (false) throw",
        ),
        (
            "</head>",
            "<style>[hidden]{display:none!important}</style>\n</head>",
        ),
    ];
    for (from, to) in swaps {
        if !s.contains(from) {
            return Err(format!("page no longer contains expected text: {from}"));
        }
        s = s.replace(from, to);
    }
    Ok(s)
}

#[get("/")]
fn index(cfg: &State<Config>) -> RawHtml<String> {
    RawHtml(cfg.page.clone())
}

#[get("/healthz")]
fn healthz() -> &'static str {
    "ok\n"
}

/// Streamed pass-through response.
struct Proxied {
    status: Status,
    content_type: Option<ContentType>,
    body: reqwest::Response,
}

impl<'r> rocket::response::Responder<'r, 'static> for Proxied {
    fn respond_to(self, _: &'r Request<'_>) -> rocket::response::Result<'static> {
        let stream = self.body.bytes_stream().map_err(io::Error::other);
        let mut r = rocket::Response::build();
        r.status(self.status)
            .header(Header::new("Cache-Control", "no-cache"))
            // tell any proxy in front (nginx, Caddy, Cloudflare) not to buffer SSE
            .header(Header::new("X-Accel-Buffering", "no"))
            .streamed_body(StreamReader::new(stream));
        if let Some(ct) = self.content_type {
            r.header(ct);
        }
        r.ok()
    }
}

async fn proxy(
    method: reqwest::Method,
    upstream_path: String,
    fwd: Fwd,
    body: Option<Vec<u8>>,
    cfg: &Config,
) -> Result<Proxied, (Status, String)> {
    let mut url = format!("{}{}", cfg.upstream, upstream_path);
    if let Some(q) = &fwd.query {
        url.push('?');
        url.push_str(q);
    }
    let mut req = cfg.client.request(method, &url);
    match (&cfg.key, &fwd.authorization) {
        (Some(k), _) => req = req.bearer_auth(k),
        (None, Some(a)) if a.trim() != "Bearer" => req = req.header("Authorization", a),
        _ => {}
    }
    if let Some(ct) = fwd.content_type {
        req = req.header("Content-Type", ct);
    }
    if let Some(a) = fwd.accept {
        req = req.header("Accept", a);
    }
    if let Some(id) = fwd.session_id {
        req = req.header("X-Hermes-Session-Id", id);
    }
    if let Some(mut b) = body {
        if let Some(m) = &cfg.model {
            if let Ok(mut v) = serde_json::from_slice::<serde_json::Value>(&b) {
                if v.get("model").is_some() {
                    v["model"] = serde_json::Value::String(m.clone());
                    b = serde_json::to_vec(&v).unwrap_or(b);
                }
            }
        }
        req = req.body(b);
    }
    let resp = req.send().await.map_err(|e| {
        (
            Status::BadGateway,
            format!("jammy: can't reach Hermes at {}: {e}\n", cfg.upstream),
        )
    })?;
    let status = Status::from_code(resp.status().as_u16()).unwrap_or(Status::BadGateway);
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<ContentType>().ok());
    Ok(Proxied { status, content_type, body: resp })
}

#[get("/v1/<path..>")]
async fn v1_get(
    path: std::path::PathBuf,
    fwd: Fwd,
    cfg: &State<Config>,
) -> Result<Proxied, (Status, String)> {
    proxy(reqwest::Method::GET, format!("/v1/{}", path.display()), fwd, None, cfg).await
}

#[post("/v1/<path..>", data = "<data>")]
async fn v1_post(
    path: std::path::PathBuf,
    fwd: Fwd,
    data: Data<'_>,
    cfg: &State<Config>,
) -> Result<Proxied, (Status, String)> {
    let body = data
        .open(20.mebibytes())
        .into_bytes()
        .await
        .map_err(|e| (Status::BadRequest, e.to_string()))?
        .into_inner();
    proxy(reqwest::Method::POST, format!("/v1/{}", path.display()), fwd, Some(body), cfg).await
}

// ---- History: Hermes's session routes. Only what Jammy uses is exposed, nothing else under /api. ----

#[get("/api/sessions")]
async fn sessions_list(fwd: Fwd, cfg: &State<Config>) -> Result<Proxied, (Status, String)> {
    proxy(reqwest::Method::GET, "/api/sessions".into(), fwd, None, cfg).await
}

#[get("/api/sessions/<id>/messages")]
async fn session_messages(id: &str, fwd: Fwd, cfg: &State<Config>) -> Result<Proxied, (Status, String)> {
    if !safe_id(id) {
        return Err((Status::BadRequest, "jammy: bad session id\n".into()));
    }
    proxy(reqwest::Method::GET, format!("/api/sessions/{id}/messages"), fwd, None, cfg).await
}

#[patch("/api/sessions/<id>", data = "<data>")]
async fn session_rename(id: &str, fwd: Fwd, data: Data<'_>, cfg: &State<Config>) -> Result<Proxied, (Status, String)> {
    if !is_jammy_id(id) {
        return Err((Status::Forbidden, "jammy: only sessions Jammy started (jammy-...) can be changed here\n".into()));
    }
    let body = data
        .open(64.kibibytes())
        .into_bytes()
        .await
        .map_err(|e| (Status::BadRequest, e.to_string()))?
        .into_inner();
    proxy(reqwest::Method::PATCH, format!("/api/sessions/{id}"), fwd, Some(body), cfg).await
}

#[delete("/api/sessions/<id>")]
async fn session_delete(id: &str, fwd: Fwd, cfg: &State<Config>) -> Result<Proxied, (Status, String)> {
    if !is_jammy_id(id) {
        return Err((Status::Forbidden, "jammy: only sessions Jammy started (jammy-...) can be deleted here\n".into()));
    }
    proxy(reqwest::Method::DELETE, format!("/api/sessions/{id}"), fwd, None, cfg).await
}

#[launch]
fn rocket() -> _ {
    let upstream = std::env::var("HERMES_URL")
        .unwrap_or_else(|_| "http://host.docker.internal:8642".into())
        .trim_end_matches('/')
        .to_owned();
    if !(upstream.starts_with("http://") || upstream.starts_with("https://")) {
        eprintln!("jammy: HERMES_URL must start with http:// or https:// (got '{upstream}')");
        std::process::exit(1);
    }
    let key = std::env::var("HERMES_API_KEY").ok().filter(|k| !k.is_empty());
    let model = std::env::var("HERMES_MODEL").ok().filter(|m| !m.is_empty());
    let page_path = std::env::var("JAMMY_PAGE").unwrap_or_else(|_| "/app/index.html".into());
    let raw = std::fs::read_to_string(&page_path).unwrap_or_else(|e| {
        eprintln!("jammy: can't read {page_path}: {e}");
        std::process::exit(1);
    });
    let page = patch_page(&raw).unwrap_or_else(|e| {
        eprintln!("jammy: {e}");
        std::process::exit(1);
    });
    println!("jammy: proxying /v1/ and /api/sessions to {upstream}");
    if let Some(m) = &model { println!("jammy: model forced to {m}"); }
    println!(
        "jammy: {}",
        if key.is_some() { "API key is added by the server" } else { "no HERMES_API_KEY; paste the key in Jammy's Settings" }
    );

    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .expect("http client");
    let figment = rocket::Config::figment()
        .merge(("address", "0.0.0.0"))
        .merge(("port", std::env::var("ROCKET_PORT").ok().and_then(|p| p.parse::<u16>().ok()).unwrap_or(8080)))
        .merge(("log_level", "normal"));
    rocket::custom(figment)
        .manage(Config { upstream, key, model, page, client })
        .mount("/", routes![index, healthz, v1_get, v1_post, sessions_list, session_messages, session_rename, session_delete])
}
