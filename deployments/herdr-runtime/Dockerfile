# syntax=docker/dockerfile:1
FROM oven/bun:1.4.3@sha256:ec06c3b6cea04192ae6770c434f668ca41d343ad19fa6472216c7b48be39c598 AS bun
FROM node:22.23.3-bookworm-slim@sha256:c3de60bf2f9dd0ac6370e6117950ff62d6e339527e7472301c9c78a017978392

ARG DEBIAN_FRONTEND=noninteractive
ARG CODEX_VERSION=0.162.1
ARG HERDR_VERSION=0.9.5
ARG HERDR_SHA256=14d9e8bc8b4a9e0118d77c32266f76d461f09b40342eb23752960bd89f59929d

LABEL org.opencontainers.image.title="Herdr Codex runtime" \
      org.opencontainers.image.description="Dedicated SSH and Herdr runtime, isolated from Hermes" \
      io.palantirkc.owner="vps-operations" \
      io.palantirkc.herdr.version="0.9.5" \
      io.palantirkc.codex.version="0.162.1"

RUN apt-get update \
 && apt-get install --yes --no-install-recommends \
      bash build-essential ca-certificates curl git gh jq less locales \
      openssh-client openssh-server procps python3 ripgrep supervisor tini \
      tzdata unzip util-linux \
 && rm -rf /var/lib/apt/lists/* \
 && groupmod --new-name palantirkc node \
 && usermod --login palantirkc --home /home/palantirkc --move-home node \
 && printf 'en_US.UTF-8 UTF-8\n' > /etc/locale.gen \
 && locale-gen \
 && install -d --owner=1000 --group=1000 --mode=0750 \
      /home/palantirkc/code/shared/agent-platform \
 && install -d --mode=0755 /etc/ssh/authorized_keys \
 && rm -f /etc/ssh/ssh_host_*_key /etc/ssh/ssh_host_*_key.pub

COPY --from=bun /usr/local/bin/bun /usr/local/bin/bun
RUN ln -s bun /usr/local/bin/bunx \
 && curl --fail --silent --show-error --location \
      --output /tmp/herdr \
      "https://github.com/herdrdev/herdr/releases/download/v${HERDR_VERSION}/herdr-linux-x86_64" \
 && printf '%s  /tmp/herdr\n' "${HERDR_SHA256}" | sha256sum --check --strict \
 && install --mode=0755 /tmp/herdr /usr/local/bin/herdr \
 && rm /tmp/herdr \
 && npm install --global --omit=dev "@openai/codex@${CODEX_VERSION}" \
 && npm cache clean --force \
 && test "$(bun --version)" = 1.4.3 \
 && test "$(node --version)" = v22.23.3 \
 && test "$(herdr --version)" = "herdr ${HERDR_VERSION}" \
 && test "$(codex --version)" = "codex-cli ${CODEX_VERSION}"

COPY sshd_config /etc/ssh/sshd_config
COPY supervisord.conf /etc/supervisor/herdr-runtime.conf
COPY profile.sh /etc/profile.d/herdr-runtime.sh
COPY --chmod=0755 entrypoint.sh /usr/local/sbin/herdr-runtime-entrypoint
COPY --chmod=0755 healthcheck.sh /usr/local/sbin/herdr-runtime-healthcheck
COPY --chmod=0755 codex-herdr /usr/local/bin/codex-herdr
COPY --chmod=0755 herdr-codex /usr/local/bin/herdr-codex

ENV LANG=en_US.UTF-8 \
    LC_ALL=en_US.UTF-8 \
    TZ=Asia/Seoul \
    SHELL=/bin/bash \
    PATH=/home/palantirkc/.local/bin:/home/palantirkc/.bun/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

WORKDIR /home/palantirkc/code/shared/agent-platform
EXPOSE 22
STOPSIGNAL SIGTERM
ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/sbin/herdr-runtime-entrypoint"]
