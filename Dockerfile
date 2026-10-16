ARG RUST_IMAGE=mirror.gcr.io/library/rust:1-bookworm
ARG RUNTIME_IMAGE=mirror.gcr.io/library/debian:bookworm-slim

FROM ${RUST_IMAGE} AS builder
RUN apt-get update \
 && apt-get install -y --no-install-recommends libcairo2-dev libpango1.0-dev libpulse-dev libxcb1-dev \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /src
# Cache dependencies separately from the app sources.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs && cargo build --release && rm -rf src
COPY src src
RUN touch src/main.rs && cargo build --release

FROM builder AS test
RUN cargo test --release

FROM ${RUST_IMAGE} AS fonts
RUN mkdir /fonts \
 && curl -fsSL -o '/fonts/Archivo.ttf' 'https://github.com/google/fonts/raw/main/ofl/archivo/Archivo%5Bwdth,wght%5D.ttf' \
 && curl -fsSL -o '/fonts/JetBrainsMono.ttf' 'https://github.com/google/fonts/raw/main/ofl/jetbrainsmono/JetBrainsMono%5Bwght%5D.ttf'

FROM ${RUNTIME_IMAGE}
RUN apt-get update \
 && apt-get install -y --no-install-recommends libcairo2 libpango-1.0-0 libpangocairo-1.0-0 libpulse0 \
      libglib2.0-bin dconf-gsettings-backend fontconfig \
 && rm -rf /var/lib/apt/lists/*
COPY --from=fonts /fonts /usr/share/fonts/truetype/countdown
RUN fc-cache -f
COPY --from=builder /src/target/release/countdown /usr/local/bin/countdown
# Named volumes inherit this ownership on first use, so the app can save its settings.
RUN mkdir -p /data /home/countdown/.config && chown -R 1000:1000 /data /home/countdown
ENV HOME=/home/countdown COUNTDOWN_CONFIG=/data/config.json COUNTDOWN_CTL=/run/countdown/ctl
USER 1000:1000
ENTRYPOINT ["/usr/local/bin/countdown"]
