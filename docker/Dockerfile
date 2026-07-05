FROM node:24-bookworm-slim AS github-authentication-builder

USER root

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
      bash \
      ca-certificates \
      curl \
      git \
      python3 \
      patch \
    && rm -rf /var/lib/apt/lists/*

COPY github-authentication /tmp/github-authentication
RUN if /tmp/github-authentication/build.sh; then \
      :; \
    else \
      echo 'warning: failed to build env-token GitHub Authentication; official mode remains available' >&2 \
      && mkdir -p /tmp/github-authentication/dist/github-authentication-env-token; \
    fi

FROM codercom/code-server:latest

USER root

ARG NODE_VERSION=24
ENV FNM_DIR=/opt/fnm/node
ENV PATH=/opt/fnm:/home/coder/.local/share/npm-global/bin:$PATH

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
      bash \
      ca-certificates \
      curl \
      git \
      python3 \
      python3-pip \
      python-is-python3 \
      sudo \
      unzip \
    && rm -rf /var/lib/apt/lists/*

RUN mkdir -p "$FNM_DIR" \
    && curl -fsSL https://fnm.vercel.app/install | bash -s -- --install-dir /opt/fnm \
    && ln -sf /opt/fnm/fnm /usr/local/bin/fnm \
    && fnm install ${NODE_VERSION} \
    && fnm default ${NODE_VERSION} \
    && NODE_BIN_DIR="$(dirname "$(fnm exec --using ${NODE_VERSION} command -v node)")" \
    && ln -sf "$NODE_BIN_DIR/node" /usr/local/bin/node \
    && ln -sf "$NODE_BIN_DIR/npm" /usr/local/bin/npm \
    && ln -sf "$NODE_BIN_DIR/npx" /usr/local/bin/npx \
    && chmod -R a+rX /opt/fnm

RUN usermod -aG sudo coder \
    && echo 'coder ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers.d/coder \
    && chmod 0440 /etc/sudoers.d/coder \
    && chmod 0777 /root \
    && mkdir -p /home/coder/.local/share/npm-global \
    && printf 'prefix=/home/coder/.local/share/npm-global\n' > /home/coder/.npmrc \
    && chown -R coder:coder /home/coder/.local /home/coder/.npmrc

COPY --chown=coder:coder chat-language-model-loader.vsix /tmp/chat-language-model-loader.vsix
COPY --from=github-authentication-builder /tmp/github-authentication/dist/github-authentication-env-token /opt/github-authentication/env-token

COPY entrypoint.sh /usr/local/bin/workspace-entrypoint
COPY git-credential-workspace-manager /usr/local/bin/git-credential-workspace-manager
RUN chmod +x /usr/local/bin/workspace-entrypoint /usr/local/bin/git-credential-workspace-manager \
    && mkdir -p /opt/github-authentication \
    && cp -a /usr/lib/code-server/lib/vscode/extensions/github-authentication /opt/github-authentication/official \
    && mkdir -p /home/coder/project \
    && chown -R coder:coder /home/coder/project

USER coder
RUN code-server --install-extension vscode-icons-team.vscode-icons \
    && code-server --install-extension ms-python.python \
    && code-server --install-extension ms-python.black-formatter \
    && code-server --install-extension /tmp/chat-language-model-loader.vsix \
    && rm -f /tmp/chat-language-model-loader.vsix

WORKDIR /home/coder/project

ENTRYPOINT ["/usr/local/bin/workspace-entrypoint"]
