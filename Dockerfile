# syntax=docker/dockerfile:1.7

ARG RUST_VERSION=1.97.1
ARG DEBIAN_RELEASE=bookworm

FROM rust:${RUST_VERSION}-${DEBIAN_RELEASE} AS development

ARG DEBIAN_FRONTEND=noninteractive
ARG SPIKE_VERSION=v1.1.0
ARG CARGO_LLVM_COV_VERSION=0.6.16
ARG DEV_USER=developer
ARG DEV_UID=1000
ARG DEV_GID=1000

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

# Host build tools, guest C/C++/assembly toolchain, and Spike build dependencies.
RUN apt-get update \
    && apt-get install --yes --no-install-recommends \
        autoconf \
        automake \
        bash-completion \
        binutils-riscv64-unknown-elf \
        build-essential \
        ca-certificates \
        clang \
        cmake \
        curl \
        device-tree-compiler \
        g++ \
        gawk \
        gcc \
        gcc-riscv64-unknown-elf \
        gdb \
        git \
        less \
        libboost-regex-dev \
        libboost-system-dev \
        libboost-thread-dev \
        libexpat1-dev \
        libtool \
        make \
        ninja-build \
        pkg-config \
        python3 \
        python3-pip \
        sudo \
        xz-utils \
        zlib1g-dev \
    && rm -rf /var/lib/apt/lists/*

# Build the reference ISS from a named upstream release. /opt/riscv is also the
# conventional prefix used by RISC-V development tools.
RUN git clone --branch "${SPIKE_VERSION}" --depth 1 \
        https://github.com/riscv-software-src/riscv-isa-sim.git /tmp/riscv-isa-sim \
    && mkdir /tmp/riscv-isa-sim/build \
    && cd /tmp/riscv-isa-sim/build \
    && ../configure --prefix=/opt/riscv \
    && make -j"$(nproc)" \
    && make install \
    && rm -rf /tmp/riscv-isa-sim

# Match the repository quality gate and coverage workflow.
RUN rustup component add clippy rustfmt llvm-tools-preview \
    && cargo install cargo-llvm-cov \
        --version "${CARGO_LLVM_COV_VERSION}" \
        --locked

# Use a non-root account by default so bind-mounted build artifacts remain
# owned by the host developer on the common UID/GID 1000 setup. Both IDs are
# configurable at build time for other hosts.
RUN groupadd --gid "${DEV_GID}" "${DEV_USER}" \
    && useradd --uid "${DEV_UID}" --gid "${DEV_GID}" \
        --create-home --shell /bin/bash "${DEV_USER}" \
    && echo "${DEV_USER} ALL=(ALL) NOPASSWD:ALL" > "/etc/sudoers.d/${DEV_USER}" \
    && chmod 0440 "/etc/sudoers.d/${DEV_USER}" \
    && mkdir -p "/home/${DEV_USER}/.cargo" /workspace \
    && chown -R "${DEV_USER}:${DEV_USER}" "/home/${DEV_USER}" /workspace

ENV RISCV=/opt/riscv
ENV RUSTUP_HOME=/usr/local/rustup
ENV CARGO_HOME=/home/${DEV_USER}/.cargo
ENV PATH=/home/${DEV_USER}/.cargo/bin:/usr/local/cargo/bin:/opt/riscv/bin:${PATH}
ENV CARGO_TERM_COLOR=always
ENV RISCV_PREFIX=riscv64-unknown-elf-

USER ${DEV_USER}
WORKDIR /workspace

# Fail the image build if one of the baseline tool families is missing.
RUN rustc --version \
    && cargo --version \
    && cargo clippy --version \
    && cargo llvm-cov --version \
    && riscv64-unknown-elf-gcc --version \
    && riscv64-unknown-elf-g++ --version \
    && riscv64-unknown-elf-as --version \
    && spike --help >/dev/null

LABEL org.opencontainers.image.source="https://github.com/mimiqdev/ruscv-sim"
LABEL org.opencontainers.image.description="Development and verification environment for ruscv-sim"
LABEL org.opencontainers.image.licenses="MIT"

CMD ["bash"]
