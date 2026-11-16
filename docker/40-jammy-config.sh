#!/bin/sh
# Writes the nginx site config at container start from HERMES_URL / HERMES_API_KEY.
set -eu

upstream="${HERMES_URL%/}"
case "$upstream" in
  http://*|https://*) ;;
  *) echo "jammy: HERMES_URL must start with http:// or https:// (got '$HERMES_URL')" >&2; exit 1 ;;
esac

if [ -n "${HERMES_API_KEY:-}" ]; then
  auth_line="proxy_set_header Authorization \"Bearer ${HERMES_API_KEY}\";"
  echo "window.JAMMY_KEY_ON_SERVER = true;" > /usr/share/nginx/html/jammy-config.js
  echo "jammy: API key is added by the proxy; the browser doesn't need it"
else
  auth_line="proxy_set_header Authorization \$http_authorization;"
  echo "window.JAMMY_KEY_ON_SERVER = false;" > /usr/share/nginx/html/jammy-config.js
  echo "jammy: no HERMES_API_KEY set; paste the key in Jammy's Settings"
fi

cat > /etc/nginx/conf.d/jammy.conf <<CONF
server {
  listen 80;
  server_name _;
  root /usr/share/nginx/html;
  index index.html;

  location = /healthz { access_log off; return 200 "ok\n"; }

  location = /jammy-config.js { add_header Cache-Control "no-store"; }

  location / { try_files \$uri /index.html; }

  # Hermes OpenAI-compatible API: /v1/chat/completions, /v1/models
  location /v1/ {
    proxy_pass ${upstream};
    proxy_http_version 1.1;
    proxy_set_header Connection "";
    proxy_set_header Host \$proxy_host;
    ${auth_line}
    # Stream tokens through as they arrive
    proxy_buffering off;
    proxy_cache off;
    gzip off;
    proxy_read_timeout 1h;
    proxy_send_timeout 1h;
    client_max_body_size 20m;
  }
}
CONF
echo "jammy: proxying /v1/ to ${upstream}"
