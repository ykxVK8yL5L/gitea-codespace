FROM codercom/code-server:latest

USER root

ARG NODE_VERSION=20

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
      bash \
      ca-certificates \
      curl \
      git \
      sudo \
      unzip \
    && rm -rf /var/lib/apt/lists/*

RUN curl -fsSL https://fnm.vercel.app/install | bash -s -- --install-dir /opt/fnm \
    && ln -s /opt/fnm/fnm /usr/local/bin/fnm \
    && fnm install ${NODE_VERSION} \
    && fnm default ${NODE_VERSION}

RUN usermod -aG sudo coder \
    && echo 'coder ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers.d/coder \
    && chmod 0440 /etc/sudoers.d/coder \
    && chmod 0777 /root

COPY --chown=coder:coder chat-language-model-loader.vsix /tmp/chat-language-model-loader.vsix

COPY entrypoint.sh /usr/local/bin/workspace-entrypoint
COPY git-credential-workspace-manager /usr/local/bin/git-credential-workspace-manager
RUN chmod +x /usr/local/bin/workspace-entrypoint /usr/local/bin/git-credential-workspace-manager \
    && mkdir -p /home/coder/project \
    && chown -R coder:coder /home/coder/project

USER coder
RUN code-server --install-extension vscode-icons-team.vscode-icons \
    && code-server --install-extension ms-python.python \
    && code-server --install-extension ms-python.black-formatter \
    && code-server --install-extension Continue.continue \
    && code-server --install-extension /tmp/chat-language-model-loader.vsix \
    && rm -f /tmp/chat-language-model-loader.vsix

WORKDIR /home/coder/project

ENTRYPOINT ["/usr/local/bin/workspace-entrypoint"]
