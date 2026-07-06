FROM node:24-bookworm-slim AS github-authentication-builder

USER root

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
      bash \
      ca-certificates \
      curl \
      git \
      python3 \
    && rm -rf /var/lib/apt/lists/*

COPY github-authentication /tmp/github-authentication
RUN bash /tmp/github-authentication/build.sh

FROM codercom/code-server:4.127.0-trixie

USER root

ARG NODE_VERSION=24
ARG RUST_VERSION=stable
ARG GO_VERSION=1.24.4
ENV FNM_DIR=/opt/fnm/node
ENV RUSTUP_HOME=/usr/local/rustup
ENV CARGO_HOME=$HOME/.cargo
ENV PATH=/opt/fnm:/usr/local/cargo/bin:/usr/local/go/bin:/usr/local/share/npm-global/bin:$PATH

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
      bash \
    build-essential \
      ca-certificates \
      curl \
      jq \
      git \
      python3 \
      python3-pip \
      python-is-python3 \
      sudo \
      unzip \
    && rm -rf /var/lib/apt/lists/*

RUN set -eux; \
        curl -fsSL https://sh.rustup.rs -o /tmp/rustup-init.sh; \
        sh /tmp/rustup-init.sh -y --no-modify-path --profile minimal --default-toolchain ${RUST_VERSION}; \
        rm -f /tmp/rustup-init.sh; \
        chmod -R a+rX ${RUSTUP_HOME} ${CARGO_HOME}; \
    ln -sf ${CARGO_HOME}/bin/cargo /usr/local/bin/cargo; \
    ln -sf ${CARGO_HOME}/bin/rustc /usr/local/bin/rustc; \
    ln -sf ${CARGO_HOME}/bin/rustdoc /usr/local/bin/rustdoc; \
    ln -sf ${CARGO_HOME}/bin/rustup /usr/local/bin/rustup; \
        rustup --version; \
        rustc --version; \
        cargo --version; \
        mkdir -p ${CARGO_HOME}/registry ${CARGO_HOME}/git; \
        chmod -R a+rwX ${RUSTUP_HOME} ${CARGO_HOME}

RUN set -eux; \
        arch="$(dpkg --print-architecture)"; \
        case "$arch" in \
            amd64) go_arch='amd64' ;; \
            arm64) go_arch='arm64' ;; \
            armhf) go_arch='armv6l' ;; \
            *) echo "Unsupported Go architecture: $arch" >&2; exit 1 ;; \
        esac; \
        curl -fsSL "https://go.dev/dl/go${GO_VERSION}.linux-${go_arch}.tar.gz" -o /tmp/go.tar.gz; \
        rm -rf /usr/local/go; \
        tar -C /usr/local -xzf /tmp/go.tar.gz; \
        rm -f /tmp/go.tar.gz; \
        ln -sf /usr/local/go/bin/go /usr/local/bin/go; \
        ln -sf /usr/local/go/bin/gofmt /usr/local/bin/gofmt; \
        go version

RUN mkdir -p "$FNM_DIR" \
    && curl -fsSL https://fnm.vercel.app/install | bash -s -- --install-dir /opt/fnm \
    && ln -sf /opt/fnm/fnm /usr/local/bin/fnm \
    && fnm install ${NODE_VERSION} \
    && fnm default ${NODE_VERSION} \
    && NODE_BIN_DIR="$(dirname "$(fnm exec --using ${NODE_VERSION} node -p 'process.execPath')")" \
    && ln -sf "$NODE_BIN_DIR/node" /usr/local/bin/node \
    && ln -sf "$NODE_BIN_DIR/npm" /usr/local/bin/npm \
    && ln -sf "$NODE_BIN_DIR/npx" /usr/local/bin/npx \
    && ln -sf "$NODE_BIN_DIR/corepack" /usr/local/bin/corepack \
    && corepack enable \
    && corepack prepare pnpm@latest --activate \
    && pnpm --version \
    && printf 'export FNM_DIR=/opt/fnm/node\nexport RUSTUP_HOME=/usr/local/rustup\nexport CARGO_HOME=/usr/local/cargo\nexport npm_config_prefix=/usr/local/share/npm-global\nexport PATH=/opt/fnm:/usr/local/cargo/bin:/usr/local/go/bin:/usr/local/share/npm-global/bin:$PATH\ncase "$-" in *i*) eval "$(fnm env --shell bash)" ;; esac\n' > /etc/profile.d/fnm.sh \
    && chmod -R a+rX /opt/fnm

RUN usermod -aG sudo coder \
    && echo 'coder ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers.d/coder \
    && chmod 0440 /etc/sudoers.d/coder \
    && chmod 0777 /root \
    && mkdir -p /usr/local/share/npm-global/bin /home/coder/.local/share/npm-global \
    && printf 'prefix=/usr/local/share/npm-global\n' > /home/coder/.npmrc \
    && printf 'prefix=/usr/local/share/npm-global\n' > /etc/npmrc \
    && chmod -R a+rwX /usr/local/share/npm-global /home/coder/.local \
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
