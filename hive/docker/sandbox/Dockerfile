# Hive Agent Sandbox
# Minimal image with common dev tools for isolated code execution.
#
# Build:  docker build -t hive-sandbox:latest hive/docker/sandbox/
# Usage:  Managed automatically by AgentSandbox in hive_terminal.

FROM ubuntu:22.04

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    ca-certificates \
    curl \
    git \
    jq \
    python3 \
    python3-pip \
    ripgrep \
    unzip \
    wget \
    && rm -rf /var/lib/apt/lists/*

# Node.js (LTS)
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && rm -rf /var/lib/apt/lists/*

# Rust toolchain (minimal)
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable --profile minimal
ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /workspace

CMD ["sleep", "infinity"]
