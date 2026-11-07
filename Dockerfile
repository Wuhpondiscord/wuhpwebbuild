FROM node:22-bookworm-slim AS web
WORKDIR /build
COPY web/package*.json ./
RUN npm ci
COPY web/ ./
RUN npm run build

FROM rust:1.99.1-bookworm AS server
WORKDIR /build
COPY Cargo.toml Cargo.lock release.txt ./
COPY migrations/ migrations/
COPY src/ src/
RUN cargo build --release --locked

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --uid 1000 --create-home app
WORKDIR /home/app
COPY --from=server /build/target/release/wuhpwebbuild /usr/local/bin/wuhpwebbuild
COPY --from=web /build/dist ./web/dist
USER 1000
EXPOSE 7860
CMD ["wuhpwebbuild"]
