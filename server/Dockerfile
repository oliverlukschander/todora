# Todora's leaderboard server, as it runs on the server.
#   docker build -f server/Dockerfile -t todora-server .
#   docker run --rm -p 8787:8787 -v todora-data:/data todora-server
FROM rust:1.97.1-bookworm AS build
# The game library the server links pulls in Bevy, which wants these to build.
RUN apt-get update \
 && apt-get install -y --no-install-recommends libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev libx11-dev pkg-config \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
RUN cargo build --locked --release -p todora-server \
 && cp target/release/todora-server /todora-server

FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends libasound2 libudev1 libwayland-client0 libxkbcommon0 \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --home /data todora \
 && mkdir -p /data && chown todora /data
COPY --from=build /todora-server /usr/local/bin/todora-server
USER todora
ENV TODORA_DB=/data/todora.db TODORA_LISTEN=0.0.0.0:8787
EXPOSE 8787
VOLUME /data
# /dev/tcp is bash's, not dash's, so the check runs in bash.
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s CMD ["/bin/bash", "-c", "exec 3<>/dev/tcp/127.0.0.1/8787"]
CMD ["todora-server"]
