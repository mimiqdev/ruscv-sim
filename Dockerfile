# syntax=docker/dockerfile:1.7

ARG RUST_VERSION=1.97.1
ARG DEBIAN_RELEASE=bookworm

FROM rust:${RUST_VERSION}-${DEBIAN_RELEASE} AS development

ARG DEBIAN_FRONTEND=noninteractive
ARG SPIKE_VERSION=v1.1.0
ARG CARGO_LLVM_COV_VERSION=0.6.16

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

# Host build tools, freestanding guest C/C++/assembly toolchain, and Spike build
# dependencies. Debian's bare-metal cross compiler intentionally does not ship
# a target libc or libstdc++; guest runtime policy remains project-owned.
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

ARG DEV_USER=developer
ARG DEV_UID=1000
ARG DEV_GID=1000

# Use a non-root account by default so bind-mounted build artifacts remain
# owned by the host developer. --non-unique makes host IDs such as macOS GID 20
# usable even when Debian already assigns the numeric ID to a system account.
RUN groupadd --non-unique --gid "${DEV_GID}" "${DEV_USER}" \
    && useradd --non-unique --uid "${DEV_UID}" --gid "${DEV_USER}" \
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

# Fail the image build if one of the baseline tool families is missing. Compile
# real native C++ and freestanding RISC-V C++ programs so a version-printing
# executable cannot masquerade as a working toolchain.
RUN rustc --version \
    && cargo --version \
    && cargo clippy --version \
    && cargo llvm-cov --version \
    && riscv64-unknown-elf-gcc --version \
    && riscv64-unknown-elf-g++ --version \
    && riscv64-unknown-elf-as --version \
    && spike --help >/dev/null \
    && printf '#include <vector>\nint main() { std::vector<int> v{42}; return v[0] != 42; }\n' \
        > /tmp/native-cxx-smoke.cc \
    && g++ -std=c++17 -Wall -Werror /tmp/native-cxx-smoke.cc \
        -o /tmp/native-cxx-smoke \
    && /tmp/native-cxx-smoke \
    && printf 'extern "C" void _start() { __asm__ volatile ("ebreak"); for (;;) {} }\n' \
        > /tmp/guest-cxx-smoke.cc \
    && riscv64-unknown-elf-g++ -march=rv64i -mabi=lp64 -ffreestanding \
        -fno-exceptions -fno-rtti -nostdlib -nostartfiles \
        -Wl,-e,_start -Wl,-Ttext=0x80000000 /tmp/guest-cxx-smoke.cc \
        -o /tmp/guest-cxx-smoke.elf \
    && riscv64-unknown-elf-readelf -h /tmp/guest-cxx-smoke.elf \
        | grep --quiet 'Machine:.*RISC-V' \
    && rm -f /tmp/native-cxx-smoke.cc /tmp/native-cxx-smoke \
        /tmp/guest-cxx-smoke.cc /tmp/guest-cxx-smoke.elf

LABEL org.opencontainers.image.source="https://github.com/mimiqdev/ruscv-sim"
LABEL org.opencontainers.image.description="Development and verification environment for ruscv-sim"
LABEL org.opencontainers.image.licenses="MIT"

CMD ["bash"]
