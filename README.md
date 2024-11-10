# Partage

### Prerequisites

- [Bun](https://bun.sh/)
- [Rust](https://www.rust-lang.org/tools/install)
- [Bacon](https://dystroy.org/bacon/)

### Development

#### Backend

```bash
bacon
```

#### Frontend

```bash
cd client
bun run dev
```

### Docker

```bash
docker build -t test-partage .
docker run --rm --name test-partage -p 20000:3001 test-partage sh
```

For `docker-compose.yml`:

```yaml
services:
  test-partage:
    image: test-partage
    ports:
      - 20000:3001
    command: sh
```

### Acknowledgements

- [Axum Websockets example](https://github.com/tokio-rs/axum/blob/main/examples/websockets/src/main.rs)
- [Rust-Embed example](https://github.com/pyrossh/rust-embed/blob/master/examples/axum-spa/main.rs)
- [Chatr](https://github.com/0xLaurens/chatr) by 0xLaurens for the backend inspiration
