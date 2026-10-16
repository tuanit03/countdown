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
# Pinned google/fonts commits + checksums, so a changed upstream file fails the build.
ARG ARCHIVO_REV=95f4904fc8bcf26d3420fe315560c96417c6dec7
ARG JBMONO_REV=6e4b84c976cadb3c49a40fd9a1c203e4f7fcf2da
WORKDIR /fonts
RUN base=https://raw.githubusercontent.com/google/fonts \
 && curl -fsSL -o Archivo.ttf "$base/$ARCHIVO_REV/ofl/archivo/Archivo%5Bwdth,wght%5D.ttf" \
 && curl -fsSL -o Archivo-OFL.txt "$base/$ARCHIVO_REV/ofl/archivo/OFL.txt" \
 && curl -fsSL -o JetBrainsMono.ttf "$base/$JBMONO_REV/ofl/jetbrainsmono/JetBrainsMono%5Bwght%5D.ttf" \
 && curl -fsSL -o JetBrainsMono-OFL.txt "$base/$JBMONO_REV/ofl/jetbrainsmono/OFL.txt" \
 && printf '%s\n' \
    '0e094a7d3c7c4c25cf1310c4b30014f1dae9332220b1c2c88f4fa996f0b05053  Archivo.ttf' \
    '108b4e57c9c796d3d38d0428ca7ee39de47ad93187302718d9b2d8864b9b716b  Archivo-OFL.txt' \
    '48715a42ec242c21e9f02692891e147d022299a52e48d5e413e1a942193ffeda  JetBrainsMono.ttf' \
    'b2fe5e8987594e9ffd1d2ca52a2f5d73eb8335243893c5d6254b5ad69269591d  JetBrainsMono-OFL.txt' \
  | sha256sum -c -

FROM ${RUNTIME_IMAGE}
LABEL org.opencontainers.image.title="countdown" \
      org.opencontainers.image.description="Frameless X11 countdown timer with synthesized focus sounds" \
      org.opencontainers.image.source="https://github.com/tuanit03/countdown" \
      org.opencontainers.image.licenses="MIT AND OFL-1.1"
RUN apt-get update \
 && apt-get install -y --no-install-recommends libcairo2 libpango-1.0-0 libpangocairo-1.0-0 libpulse0 \
      libglib2.0-bin dconf-gsettings-backend gsettings-desktop-schemas fontconfig \
 && rm -rf /var/lib/apt/lists/*
COPY --from=fonts /fonts /usr/share/fonts/truetype/countdown
RUN fc-cache -f
COPY --from=builder /src/target/release/countdown /usr/local/bin/countdown
# Named volumes inherit this ownership/mode on first use, so the app can save its settings.
# World-writable (sticky) so the container also works when run as the host user's uid instead of 1000.
RUN mkdir -p /data /home/countdown/.config /home/countdown/.cache /run/countdown \
 && chown -R 1000:1000 /data /home/countdown /run/countdown \
 && chmod 1777 /data /home/countdown /home/countdown/.config /home/countdown/.cache /run/countdown \
 && head -c 256 /dev/zero > /etc/pulse-dummy-cookie && chmod 644 /etc/pulse-dummy-cookie
# PipeWire ignores the Pulse auth cookie; a dummy one keeps libpulse from complaining on a read-only fs.
ENV HOME=/home/countdown COUNTDOWN_CONFIG=/data/config.json COUNTDOWN_CTL=/run/countdown/ctl \
    PULSE_COOKIE=/etc/pulse-dummy-cookie
USER 1000:1000
ENTRYPOINT ["/usr/local/bin/countdown"]
