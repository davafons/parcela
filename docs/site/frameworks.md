# Frameworks and data

Parcela gives each checkout its own ports, processes and URLs. It does not copy, isolate or delete your data, and it knows nothing about frameworks. This page shows how to wire common stacks to the variables Parcela sets, so every worktree gets its own data and its dev server accepts its own URL.

The examples use the `signup` worktree of a project called `shop`, at `https://signup-shop.localhost:1443`.

## What a process gets

Every service, hook and task gets these, among others. The full list is in the [configuration reference](configuration.md#environment-variables-parcela-sets).

| Variable | Use it for |
|---|---|
| `PORT`, `HOST` | The port to listen on, and `127.0.0.1` |
| `PARCELA_URL` | Links, mailers, OAuth callbacks, trusted origins |
| `PARCELA_HOSTNAME` | Host checks, like `signup-shop.localhost` |
| `PARCELA_SHARE_HOSTS` | The extra hostnames after `up --lan` or `--tailscale`, comma-separated. Empty otherwise. |
| `PARCELA_SERVICE_<NAME>_URL`, `_PORT` | Another service's URL, and its backend port on 127.0.0.1 |
| `PARCELA_ENVIRONMENT_NAME` | Naming databases and key prefixes: `primary`, or `signup` |
| `PARCELA_DATA_DIR` | Files that belong to this environment |
| `PARCELA_RUNTIME_DIR` | Holds `data`, `tmp`, `cache` and `log` |
| `PARCELA_PRIMARY_DIR` | The main checkout, to copy `.env` from |

Parcela creates `tmp` and `cache` under `PARCELA_RUNTIME_DIR` but sets no variable for them, and it does not set `TMPDIR`. Point a tool at `${PARCELA_RUNTIME_DIR}/tmp` yourself if you want it there.

Values in `env` can use `${NAME}` only for variables Parcela sets. `${HOME}` or a shell variable is an error. Commands also inherit the environment of the shell that ran `parcela up`, except variables starting with `PARCELA_`.

## Data per worktree

A new worktree is a new directory, so files the app keeps inside the checkout, such as Rails' `storage/` or Django's `db.sqlite3`, already differ per worktree. They start empty and go away with the worktree. Anything outside the checkout, such as a database server, a Redis or an `.env` that points at one, is shared until `parcela.yaml` says otherwise.

`hooks.setup` runs on an environment's first `up`, `hooks.prestart` on every start and `hooks.teardown` on `parcela rm`. The [hooks reference](configuration.md#hooks) has the details.

### SQLite

Keep the file in `PARCELA_DATA_DIR`, so it survives removing the worktree:

```yaml
env:
  DATABASE_PATH: ${PARCELA_DATA_DIR}/development.sqlite3
hooks:
  setup:
    command: bin/rails db:prepare
```

Rails, in `config/database.yml`:

```yaml
development:
  adapter: sqlite3
  database: <%= ENV.fetch("DATABASE_PATH", "storage/development.sqlite3") %>
```

Django, in `settings.py`, with `python manage.py migrate` as the setup command:

```python
DATABASES = {
    "default": {
        "ENGINE": "django.db.backends.sqlite3",
        "NAME": os.environ.get("DATABASE_PATH", BASE_DIR / "db.sqlite3"),
    }
}
```

### PostgreSQL

Two ways, as in [Databases](configuration.md#databases).

**A server per environment.** Run `postgres` as a service on a named port, as in [the complete example](configuration.md#a-complete-example). Its files live in `PARCELA_DATA_DIR` and `down` stops it.

**One shared server, one database per environment.** Name the database after the environment. libpq tools such as `psql`, `createdb` and `dropdb` read `PGHOST`:

```yaml
env:
  PGHOST: 127.0.0.1
  DATABASE_URL: postgres://127.0.0.1/shop_${PARCELA_ENVIRONMENT_NAME}
hooks:
  setup:
    command: bin/rails db:prepare
  prestart:
    command: bin/rails db:migrate
  teardown:
    command: dropdb --if-exists "shop_$PARCELA_ENVIRONMENT_NAME"
```

`bin/rails db:prepare` creates the database. Django does not create databases, so do it in `setup` before migrating:

```yaml
hooks:
  setup:
    command: >-
      psql -d postgres -tAc "SELECT 1 FROM pg_database WHERE datname = 'shop_$PARCELA_ENVIRONMENT_NAME'" | grep -q 1
      || createdb "shop_$PARCELA_ENVIRONMENT_NAME";
      python manage.py migrate
```

- `teardown` runs on `parcela rm` of any environment, the main checkout included, where it drops `shop_primary`. Leave it out if that is not what you want.
- Environment names can contain hyphens. `createdb`, `dropdb` and Rails quote the name. Quote it yourself in raw SQL.
- To start a worktree from a copy of the main checkout's data, run `createdb -T shop_primary "shop_$PARCELA_ENVIRONMENT_NAME"` in `setup` instead. PostgreSQL refuses while anything is connected to `shop_primary`, so stop the main checkout first.

### Redis, Sidekiq and queues

Run a Redis per environment on a named port. Its dump goes to `PARCELA_DATA_DIR`:

```yaml
ports: [redis]

env:
  REDIS_URL: redis://127.0.0.1:${PARCELA_PORT_REDIS}/0

services:
  redis:
    command: exec redis-server --port "$PARCELA_PORT_REDIS" --bind 127.0.0.1 --dir "$PARCELA_DATA_DIR"
    healthcheck: redis-cli -p "$PARCELA_PORT_REDIS" ping
  web:
    command: bin/rails server -p $PORT -b 127.0.0.1
    http:
      healthcheck: /up
    depends_on: [redis]
  sidekiq:
    command: bundle exec sidekiq
    depends_on: [redis]
```

Sidekiq reads `REDIS_URL` when it has no URL configured. Rails' Redis cache store and Action Cable's `redis` adapter take the URL from their configuration, so pass `ENV["REDIS_URL"]` there. The generated `config/cable.yml` uses the `async` adapter in development. Two workers that share one Redis take each other's jobs. Sidekiq 7 dropped namespaces, so on a shared server the only separation is a database index per worktree, which you assign by hand. A prefix such as `shop:${PARCELA_ENVIRONMENT_NAME}:` works for caches that accept a namespace, such as the Redis cache store's `namespace:`.

Queues that live in the database, such as Solid Queue, GoodJob or Oban, are separate once the database is.

### Uploads and Active Storage

Point the upload root at `PARCELA_DATA_DIR`:

```yaml
env:
  STORAGE_ROOT: ${PARCELA_DATA_DIR}/storage
```

Rails, in `config/storage.yml`:

```yaml
local:
  service: Disk
  root: <%= ENV.fetch("STORAGE_ROOT") { Rails.root.join("storage").to_s } %>
```

Django: `MEDIA_ROOT = os.environ.get("STORAGE_ROOT", BASE_DIR / "media")`.

## Host checks

Requests reach the app through Caddy, which [keeps the browser's `Host`](https://caddyserver.com/docs/caddyfile/directives/reverse_proxy#defaults), such as `signup-shop.localhost:1443`, and sets `X-Forwarded-Proto: https`, `X-Forwarded-Host` and `X-Forwarded-For`. Dev servers that check hosts usually allow `localhost` and its subdomains, so the loopback URL works. The names from `--lan` and `--tailscale` are in `PARCELA_SHARE_HOSTS`. `--tailscale` sends requests from `tailscale serve` straight to the backend port, not through Caddy. `tailscale serve` also keeps `Host` and sets `X-Forwarded-Proto: https`.

Keep these settings in development configuration only.

### Rails

Rails 6 and later allow `.localhost` hosts in development. Add the shared names, and set URLs for mailers and jobs, which have no request to take them from. In `config/environments/development.rb`:

```ruby
config.hosts.concat(ENV.fetch("PARCELA_SHARE_HOSTS", "").split(","))

if ENV["PARCELA_URL"]
  url = URI(ENV["PARCELA_URL"])
  config.action_mailer.default_url_options = { host: url.host, port: url.port, protocol: url.scheme }
end
```

Action Cable accepts a connection whose `Origin` matches the request's scheme and `Host`, and Rack takes the scheme from `X-Forwarded-Proto`, so the page's own origin works. If it refuses one, set `config.action_cable.allowed_request_origins = [ENV["PARCELA_URL"]]`.

### Django

`ALLOWED_HOSTS` falls back to `.localhost`, `127.0.0.1` and `[::1]` only while it is empty and `DEBUG` is on, so list them once you add the shared names. Django rejects a POST whose `Origin` is `https://…` while it thinks the request is plain HTTP. Trust Caddy's header, or list the URL:

```python
SECURE_PROXY_SSL_HEADER = ("HTTP_X_FORWARDED_PROTO", "https")
ALLOWED_HOSTS = [".localhost", "127.0.0.1", "[::1]"] + [
    host for host in os.environ.get("PARCELA_SHARE_HOSTS", "").split(",") if host
]
CSRF_TRUSTED_ORIGINS = [os.environ["PARCELA_URL"]] if "PARCELA_URL" in os.environ else []
```

Start it with `python manage.py runserver 127.0.0.1:$PORT`.

### Vite

Vite [allows](https://vite.dev/config/server-options#server-allowedhosts) `localhost`, its subdomains and IP addresses. Add the shared names:

```js
server: {
  host: '127.0.0.1',
  port: Number(process.env.PORT),
  strictPort: true,
  allowedHosts: (process.env.PARCELA_SHARE_HOSTS ?? '').split(',').filter(Boolean),
},
```

HMR needs no settings. The Vite client connects back to the host and port it was loaded from, over `wss://` on an HTTPS page, and Caddy forwards the websocket. This repository's own docs site runs that way. Do not move HMR to a separate `server.hmr.port`: Parcela routes only HTTP services, so the browser would have to reach that port directly. If something already sets `server.ws.clientPort`, or `server.hmr.clientPort` in older Vite, use the proxy's port: `new URL(process.env.PARCELA_URL).port || 443`.

When Vite runs next to a backend, as with Vite Ruby or Laravel's plugin, give it its own HTTP service, so it gets a free port and its own HTTPS URL:

```yaml
services:
  vite:
    command: npx vite --host 127.0.0.1 --port $PORT --strictPort
    http:
      subdomain: vite
      healthcheck: /@vite/client
```

Set `server.origin: process.env.PARCELA_SERVICE_VITE_URL` so asset URLs point at it. The plugin decides how the page finds the dev server:

- **Laravel.** `laravel-vite-plugin` writes `server.origin` to its hot file, and `@vite` loads assets from there.
- **Vite Ruby.** Rails proxies assets to `VITE_RUBY_HOST` and `VITE_RUBY_PORT`, and the plugin sets Vite's port from them. Set them in the top-level `env`, to `127.0.0.1` and `${PARCELA_SERVICE_VITE_PORT}`. The plugin also points the HMR websocket at that port unless `clientPort` is set.

### Next.js

Next.js blocks dev-only requests from origins it does not know. It [allows](https://nextjs.org/docs/app/api-reference/config/next-config-js/allowedDevOrigins) `localhost` and its subdomains. Add the shared names, in `next.config.js`:

```js
module.exports = {
  allowedDevOrigins: (process.env.PARCELA_SHARE_HOSTS ?? '').split(',').filter(Boolean),
};
```

Start it with `next dev --port $PORT --hostname 127.0.0.1`.

### Phoenix

Set the endpoint's `url` so generated URLs use the proxy. In `config/dev.exs`:

```elixir
parcela = URI.parse(System.get_env("PARCELA_URL", "http://localhost:4000"))

config :my_app, MyAppWeb.Endpoint,
  http: [ip: {127, 0, 0, 1}, port: String.to_integer(System.get_env("PORT", "4000"))],
  url: [host: parcela.host, port: parcela.port, scheme: parcela.scheme]
```

A new app's `config/dev.exs` sets `check_origin: false`. If you turn the check on, the default compares the `Origin` host with `url`'s host, so list the shared names too. `check_origin: :conn` also compares the scheme, which is `http` behind the proxy, so it refuses the page.

### Laravel

```yaml
env:
  APP_URL: ${PARCELA_URL}
services:
  web:
    command: php artisan serve --host=127.0.0.1 --port=$PORT --no-reload
    http:
      healthcheck: /up
```

Variables already in the environment win over `.env`. When a `.env` exists, `php artisan serve` passes only a short list of them to its PHP process unless it runs with `--no-reload`. [Trust the proxy](https://laravel.com/docs/12.x/requests#configuring-trusted-proxies), `->withMiddleware(fn ($middleware) => $middleware->trustProxies(at: '127.0.0.1'))` in `bootstrap/app.php`, so generated URLs use `https`.

## Cookies

Each environment is its own host, so host-only cookies, the default, keep sessions apart: logging in to `signup-shop.localhost` does not log you in to `shop.localhost`. HTTPS URLs make `Secure` cookies work.

`localhost` is not on the [Public Suffix List](https://publicsuffix.org/), so browsers treat it as a top-level domain, and `shop.localhost` and `api-shop.localhost` are different sites. A request from one to the other is cross-site, and cookies with `SameSite=Lax`, Rails' and Django's default, are not sent with it. Either keep the browser on one origin, for example with a dev-server proxy to `http://127.0.0.1:${PARCELA_SERVICE_API_PORT}`, or use `SameSite=None; Secure` cookies with credentialed CORS.

With `wildcard: true`, tenants such as `kobe.signup-shop.localhost` are subdomains of the environment's host. A cookie for `Domain=signup-shop.localhost` reaches every tenant of that worktree and no other worktree.

## Calls between services

Server-side code can call another service on its backend port, without DNS, TLS or a certificate authority:

```yaml
env:
  API_INTERNAL_URL: http://127.0.0.1:${PARCELA_SERVICE_API_PORT}
  API_PUBLIC_URL: ${PARCELA_SERVICE_API_URL}
```

Use the public URL for anything a browser follows. The request arrives with `Host: 127.0.0.1:<port>`, which Rails and Django allow in development and the Django settings above list.

## OAuth callbacks and webhooks

Every worktree has its own origin, so its OAuth callback is its own URL, such as `https://signup-shop.localhost:1443/auth/callback`. Build it from the request or from `PARCELA_URL`, never from a fixed host.

- Most providers match redirect URIs exactly, port included, and few accept wildcards. Register the callback for each name you use. Names stay after the first `up`, and `parcela up --name NAME` picks one, so a few registered names can be reused.
- `parcela proxy install` on macOS drops the `:1443` from URLs. Register the URLs you will actually see.
- With `wildcard: true`, register one callback on the environment's host and send the tenant along in the OAuth `state`. A provider cannot be given every tenant host.
- Some providers refuse `*.localhost`. [Google](https://developers.google.com/identity/protocols/oauth2/web-server#uri-validation) requires a top-level domain on the Public Suffix List, which `localhost` is not.

`.localhost` URLs answer only on this machine, so a webhook from the internet cannot reach them. `--tailscale` serves your tailnet only, and Parcela never uses Funnel. Use the provider's forwarding CLI as a service and point it at the backend port:

```yaml
services:
  stripe:
    command: stripe listen --forward-to "http://127.0.0.1:$PARCELA_SERVICE_WEB_PORT/webhooks"
```

## Clients other than browsers

### Name resolution

Browsers resolve `localhost` and every subdomain of it to loopback without asking DNS. curl 7.85.0 and later do the same. Other programs ask the system resolver:

- **macOS.** The system resolver answers any `*.localhost` with `::1` and `127.0.0.1`, as checked on macOS 26 with Python, Node, Ruby and Go. The proxy listens on 127.0.0.1 only, so a client must fall back from `::1`; common ones do.
- **Go's own resolver** does not resolve `*.localhost`: `lookup foo.localhost: no such host` on Go 1.27 with `GODEBUG=netdns=go`. On Linux, Go hands `*.localhost` to glibc only when `/etc/nsswitch.conf` lists `myhostname` or `resolve` for hosts and the binary has cgo. A `CGO_ENABLED=0` binary always uses its own.
- **Linux.** glibc resolves `*.localhost` only through systemd-resolved or [`nss-myhostname`](https://www.freedesktop.org/software/systemd/man/latest/nss-myhostname.html). Without them, add the names to `/etc/hosts`, or call the backend port.

For calls from the app itself, the backend port in `PARCELA_SERVICE_<NAME>_PORT` avoids the question.

### The certificate authority

`parcela trust` adds Parcela's certificate authority to the system trust store. Programs with their own CA list do not see it. `parcela trust --print` prints the root certificate's path and changes nothing. It is in the state directory, by default `~/.local/state/parcela/proxy/data/pki/authorities/local/root.crt`, and exists once the proxy has started.

| Client | Variable | Effect |
|---|---|---|
| Node | `NODE_EXTRA_CA_CERTS` | Adds to Node's built-in list. Read at startup. |
| Node 22.19+, 24.6+ | `NODE_USE_SYSTEM_CA=1` | Also trusts the system store, so `parcela trust` is enough. Same as `--use-system-ca`. |
| Python `requests` | `REQUESTS_CA_BUNDLE` | Replaces certifi's bundle. |
| Python httpx | `SSL_CERT_FILE` | Replaces certifi's bundle. |
| Python `ssl`, `urllib` | `SSL_CERT_FILE` | Replaces OpenSSL's default file. |
| Ruby `Net::HTTP` | `SSL_CERT_FILE` | Replaces OpenSSL's default file. |
| Go | `SSL_CERT_FILE` | Replaces the default file on Linux. On macOS Go uses the system store, so `parcela trust` is enough; from Go 1.27, setting the variable there replaces the store too. |
| curl | `--cacert` | `curl --cacert "$(parcela trust --print)" "$(parcela url)"` |

A variable that replaces the bundle breaks every other HTTPS call, so give it a combined bundle:

```sh
cat "$(python3 -m certifi)" "$(parcela trust --print)" > ~/.local/share/dev-ca.pem
export REQUESTS_CA_BUNDLE=~/.local/share/dev-ca.pem
```

Export the variable in the shell that runs `parcela up`, since commands inherit it. To set it in `parcela.yaml` instead, build the path from the runtime directory, which sits in the same state directory:

```yaml
env:
  NODE_EXTRA_CA_CERTS: ${PARCELA_RUNTIME_DIR}/../../proxy/data/pki/authorities/local/root.crt
```

Inside a service, `parcela trust --print` does not see a `PARCELA_STATE_DIR` from your shell, because Parcela removes inherited `PARCELA_` variables. The path above follows any state directory.
