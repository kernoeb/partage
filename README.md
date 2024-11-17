# Partage

![Workflow](https://github.com/kernoeb/partage/actions/workflows/docker-publish.yml/badge.svg)

![capture partage](resources/partage.png)

### Prerequisites

- [Bun](https://bun.sh/)
- [Rust](https://www.rust-lang.org/tools/install)
- [Bacon](https://dystroy.org/bacon/)
- [Sqlx CLI](https://github.com/launchbadge/sqlx/blob/main/sqlx-cli/README.md)

### Development

#### Backend

- Sqlx setup

```bash
sqlx database create
sqlx migrate run
```

- Start the backend server

```bash
bacon
```

- Generate types (bindings) for the frontend

```bash
cargo test
```

#### Frontend

```bash
cd client
bun run dev
```

Open [http://localhost:13124](http://localhost:13124) in your browser.

### Build

### Linux, MacOS

```bash
cd client
bun run build
cd ..
cargo build --release
```

> By default, Rust will build for the host architecture. To build for another architecture, use the `--target` flag.
> 
> :warning: Rust leak your username and your current directory in the binary. To avoid this, look at the [Arm/Raspberry Pi](#armraspberry-pi) section, or the [Dockerfile](Dockerfile).

#### Arm/Raspberry Pi

- Install [cross](https://github.com/cross-rs/cross/)

```bash
CROSS_CONTAINER_OPTS="--platform linux/amd64 -e RUSTFLAGS='-Zlocation-detail=none -Zfmt-debug=shallow'" cross +nightly build \
-Z build-std=std,panic_abort \
-Z build-std-features=panic_immediate_abort \
--target armv7-unknown-linux-musleabihf \
--release
```

#### Docker

```bash
docker build -t test-partage .
docker run --rm --name test-partage -p 20000:3001 test-partage sh
```

For `docker-compose.yml`:

```yaml
services:
  partage:
    image: ghcr.io/kernoeb/partage:main
    ports:
      - 20000:3001
```

### Deployment

#### Nginx

```sh
certbot -d x.example.com --manual --preferred-challenges dns certonly
```

```nginx
server {
  listen 443 ssl http2;
  listen [::]:443 ssl http2;
  server_name x.example.com;

  ssl_certificate /etc/letsencrypt/live/x.example.com/fullchain.pem;
  ssl_certificate_key /etc/letsencrypt/live/x.example.com/privkey.pem;

  ssl_protocols TLSv1.2 TLSv1.3;
  ssl_prefer_server_ciphers on;
  ssl_ciphers "EECDH+AESGCM:EDH+AESGCM:AES256+EECDH:AES256+EDH";

  gzip on;
  gzip_disable "msie6";

  gzip_types text/plain text/css application/json application/javascript application/x-javascript text/xml application/xml application/xml+rss text/javascript;

  location / {
      proxy_pass http://localhost:21000;
      proxy_http_version 1.1;
      proxy_set_header Upgrade $http_upgrade;
      proxy_set_header Connection "upgrade";
      proxy_set_header Host $host;
      proxy_set_header X-Real-IP $remote_addr;
      proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
      proxy_set_header X-Forwarded-Proto $scheme;
  }
}

server {
  listen 80;
  server_name x.example.com;

  # Redirect HTTP to HTTPS
  return 301 https://$host$request_uri;
}
```

### Acknowledgements

- [Axum Websockets example](https://github.com/tokio-rs/axum/blob/main/examples/websockets/src/main.rs)
- [Rust-Embed example](https://github.com/pyrossh/rust-embed/blob/master/examples/axum-spa/main.rs)
- [Chatr](https://github.com/0xLaurens/chatr) by 0xLaurens for the backend inspiration

### TODO

- [x] Tests
- [ ] Documentation
- [x] Persistence for new channels
- [x] Feature : no database
