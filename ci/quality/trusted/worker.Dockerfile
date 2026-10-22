FROM node:22.23.3-bookworm@sha256:d05fa967fa1bbe6ba4fb48f9d4c55b5e6a8c8300aedb6b99a11c248a877d7bee
# The trusted workflow builds this immutable recipe before any candidate execution.
# No repository, credential, native home or Docker socket enters the image.
RUN curl --fail --show-error --location --proto '=https' --proto-redir '=https' --tlsv1.2 \
      https://github.com/oven-sh/bun/releases/download/bun-v1.4.3/bun-linux-x64.zip \
      --output /tmp/bun.zip \
    && echo '3e730528d89775f03f87f05c06787c4df4099d0a37f3f252e7b829cb23816e18  /tmp/bun.zip' | sha256sum --check --strict \
    && unzip -p /tmp/bun.zip bun-linux-x64/bun > /usr/local/bin/bun \
    && echo '7ce7d6b654eddeec20afddf2396317fba6dc19ab52788a27e894e1806bf8f24b  /usr/local/bin/bun' | sha256sum --check --strict \
    && chmod 0555 /usr/local/bin/bun \
    && rm /tmp/bun.zip
RUN mkdir /prepared && chown 1000:1000 /prepared
ENV HOME=/job/home LANG=C.UTF-8 CI=true RUNNER_TEMP=/job/tmp
USER 1000:1000
WORKDIR /job
ENTRYPOINT ["/usr/bin/env", "-i", "HOME=/job/home", "PATH=/usr/local/bin:/usr/bin:/bin", "LANG=C.UTF-8", "CI=true", "RUNNER_TEMP=/job/tmp"]
