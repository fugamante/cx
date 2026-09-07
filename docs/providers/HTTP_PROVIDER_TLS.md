# HTTP Provider TLS Policy

`xshelf` supports an optional HTTP adapter path (`CX_PROVIDER_ADAPTER=http-curl`).

Auth profiles on the same boundary:
- `CX_HTTP_AUTH_PROFILE=bearer` (default): uses `CX_HTTP_PROVIDER_TOKEN` as `Authorization: Bearer ...`
- `CX_HTTP_AUTH_PROFILE=basic`: uses:
  - `CX_HTTP_AUTH_USERNAME`
  - `CX_HTTP_AUTH_PASSWORD`
- `CX_HTTP_AUTH_PROFILE=header`: uses:
  - `CX_HTTP_AUTH_HEADER`
  - `CX_HTTP_AUTH_VALUE` (or falls back to `CX_HTTP_PROVIDER_TOKEN`)

Secret file sources on the same boundary:
- `CX_HTTP_PROVIDER_TOKEN_FILE`
- `CX_HTTP_AUTH_VALUE_FILE`
- `CX_HTTP_AUTH_PASSWORD_FILE`

Rules:
- set only one of `FOO` or `FOO_FILE` for a given secret source
- secret files must not be group/world readable or writable on Unix
- file contents are trimmed before use

## Runtime policy (in-process)

- `CX_HTTP_REQUIRE_HTTPS=1` (default): blocks non-HTTPS provider URLs.
- `CX_HTTP_ALLOW_LOCAL_HTTP=1` (default): allows plain HTTP only for loopback hosts:
  - `http://localhost`
  - `http://127.0.0.1`
  - `http://[::1]`
- `CX_HTTP_ALLOWED_HOSTS` (optional CSV): host allowlist gate for `CX_HTTP_PROVIDER_URL`.
- `CX_HTTP_TLS_PINNEDPUBKEY` (optional): passed to curl `--pinnedpubkey` for TLS pinning.
- `CX_HTTP_CA_BUNDLE` (optional): passed to curl `--cacert` for custom trust bundles.
- `CX_HTTP_CLIENT_CERT` / `CX_HTTP_CLIENT_KEY` (optional): passed to curl `--cert` / `--key` for mTLS.
- `CX_HTTP_TLS_MIN_VERSION` (optional): explicit TLS version floor:
  - `1.2` (default)
  - `1.3`
  - `default` to defer to system curl defaults
- `CX_HTTP_FOLLOW_REDIRECTS` (optional, default `0`): opt into HTTP redirects.
- `CX_HTTP_MAX_REDIRECTS` (optional, default `3` when redirects are enabled): redirect cap.

Behavior:
- `https://...` always allowed.
- `http://...` non-loopback is rejected by default.
- Set `CX_HTTP_REQUIRE_HTTPS=0` only for controlled local testing.

## Deployment pattern (out-of-process TLS termination)

Use TLS at the edge and point XSHELF to the HTTPS endpoint.

### Caddy example

```caddyfile
llm.example.com {
  reverse_proxy 127.0.0.1:8081
}
```

Set:

```bash
export CX_PROVIDER_ADAPTER=http-curl
export CX_HTTP_PROVIDER_URL=https://llm.example.com/v1/chat
export CX_HTTP_REQUIRE_HTTPS=1
export CX_HTTP_TLS_MIN_VERSION=1.2
```

### Nginx example

```nginx
server {
  listen 443 ssl;
  server_name llm.example.com;

  ssl_certificate /etc/letsencrypt/live/llm.example.com/fullchain.pem;
  ssl_certificate_key /etc/letsencrypt/live/llm.example.com/privkey.pem;

  location / {
    proxy_pass http://127.0.0.1:8081;
    proxy_set_header Host $host;
    proxy_set_header X-Forwarded-Proto https;
  }
}
```

## Operator checks

```bash
./bin/xshelf version | rg 'provider_transport|http_require_https|http_allow_local_http'
./bin/xshelf core | rg 'provider_transport|http_require_https|http_allow_local_http'
./bin/xshelf version | rg 'http_tls_posture|http_tls_min_version|http_follow_redirects'
./bin/xshelf core | rg 'http_tls_posture|http_tls_min_version|http_follow_redirects'
```
