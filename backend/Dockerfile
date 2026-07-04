FROM rust:1.88-slim AS builder
WORKDIR /app
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && cargo build --release && rm -rf src target/release/deps/wealth_agent_backend*
COPY src ./src
COPY migrations ./migrations
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates wget && rm -rf /var/lib/apt/lists/*
RUN useradd -r -u 1001 -s /usr/sbin/nologin wealthagent
WORKDIR /app
COPY --from=builder /app/target/release/wealth-agent-backend /usr/local/bin/wealth-agent-backend
COPY --from=builder /app/target/release/admin /usr/local/bin/admin
COPY --from=builder /app/migrations /app/migrations
COPY entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh
USER 1001
ENV BIND_ADDR=0.0.0.0:8080
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
