# syntax=docker/dockerfile:1.7

ARG RUST_VERSION=1.97.1
ARG DEBIAN_RELEASE=bookworm

FROM rust:${RUST_VERSION}-${DEBIAN_RELEASE} AS development

ARG DEBIAN_FRONTEND=noninteractive
# Commit 5d996117 (riscv-isa-sim PR 2086) implements abstract Access Memory
# and already advertises --dm-datacount, --halted, and --rbb-port. A later
# tag does not exist; v1.1.0 predates those flags.
ARG SPIKE_VERSION=5d99611782ef8dd81b65b842369bd351a9d4702a
# Accellera SystemC 3.0.2 at the commit the blocking-transport contract pins
# (dev-plan D9). That value is a commit SHA and the head of Accellera's
# `release` branch, so it is fetched explicitly instead of through --branch.
ARG SYSTEMC_VERSION=70b0fc8e4a74acc677b0fc73cea08f940c2115d5
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
        gdb-multiarch \
        git \
        less \
        libboost-regex-dev \
        libboost-system-dev \
        libboost-thread-dev \
        libexpat1-dev \
        libtool \
        make \
        ninja-build \
        openocd \
        pkg-config \
        python3 \
        python3-pip \
        sudo \
        xz-utils \
        zlib1g-dev \
    && rm -rf /var/lib/apt/lists/*

# Build the reference ISS from SPIKE_VERSION. That value is a commit SHA.
# `git clone --branch` does not accept a raw SHA, so fetch it explicitly.
# The patch keeps `--dm-progsize=0 --dm-no-impebreak` from aborting in the
# debug-module constructor. Upstream still has that assert on master.
# /opt/riscv is also the conventional prefix used by RISC-V development tools.
COPY scripts/debug/spike-5d996117-empty-progbuf.patch /tmp/spike-5d996117-empty-progbuf.patch
RUN git init /tmp/riscv-isa-sim \
    && git -C /tmp/riscv-isa-sim remote add origin https://github.com/riscv-software-src/riscv-isa-sim.git \
    && git -C /tmp/riscv-isa-sim fetch --depth 1 origin "${SPIKE_VERSION}" \
    && git -C /tmp/riscv-isa-sim checkout --detach FETCH_HEAD \
    && patch -p1 -d /tmp/riscv-isa-sim < /tmp/spike-5d996117-empty-progbuf.patch \
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

# Build the pinned SystemC kernel and its TLM-2.0 headers. SystemC 3.0 needs
# C++17; selecting it here also makes the installed SystemCLanguage package
# advertise C++17 to consumers. A static library keeps the integration smoke
# free of runtime loader search paths. The checked-out commit and the version
# macros are verified before anything is configured, so a moved or mislabeled
# pin fails the image build instead of silently changing the kernel.
RUN git init /tmp/systemc \
    && git -C /tmp/systemc remote add origin https://github.com/accellera-official/systemc.git \
    && git -C /tmp/systemc fetch --depth 1 origin "${SYSTEMC_VERSION}" \
    && git -C /tmp/systemc checkout --detach FETCH_HEAD \
    && test "$(git -C /tmp/systemc rev-parse HEAD)" = "${SYSTEMC_VERSION}" \
    && grep -qx '#define SYSTEMC_3_0_2' /tmp/systemc/src/sysc/kernel/sc_ver.h \
    && cmake -S /tmp/systemc -B /tmp/systemc/build -G Ninja \
        -DCMAKE_BUILD_TYPE=Release \
        -DCMAKE_CXX_STANDARD=17 \
        -DCMAKE_INSTALL_PREFIX=/opt/systemc \
        -DBUILD_SHARED_LIBS=OFF \
        -DENABLE_EXAMPLES=OFF \
        -DENABLE_REGRESSION=OFF \
    && cmake --build /tmp/systemc/build --parallel "$(nproc)" \
    && cmake --install /tmp/systemc/build \
    && test -f /opt/systemc/include/sysc/kernel/sc_ver.h \
    && rm -rf /tmp/systemc

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
ENV SYSTEMC_HOME=/opt/systemc
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
    && openocd --version \
    && gdb-multiarch --version \
    && spike --help > /tmp/spike-help.txt 2>&1 \
    && grep -q -- '--dm-datacount' /tmp/spike-help.txt \
    && grep -q -- '--halted' /tmp/spike-help.txt \
    && rm -f /tmp/spike-help.txt \
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
        /tmp/guest-cxx-smoke.cc /tmp/guest-cxx-smoke.elf \
    && printf '#include <systemc>\n#include <tlm>\nint sc_main(int, char**) {\n    sc_core::sc_set_time_resolution(1, sc_core::SC_FS);\n    tlm::tlm_generic_payload payload;\n    sc_core::sc_time delay = sc_core::SC_ZERO_TIME;\n    return payload.get_response_status() == tlm::TLM_INCOMPLETE_RESPONSE\n               && delay == sc_core::SC_ZERO_TIME\n           ? 0\n           : 1;\n}\n' \
        > /tmp/systemc-smoke.cpp \
    && g++ -std=c++17 -Wall -Werror -isystem /opt/systemc/include \
        /tmp/systemc-smoke.cpp /opt/systemc/lib/libsystemc.a \
        -lpthread -ldl -lm \
        -o /tmp/systemc-smoke \
    && /tmp/systemc-smoke \
    && rm -f /tmp/systemc-smoke.cpp /tmp/systemc-smoke

LABEL org.opencontainers.image.source="https://github.com/mimiqdev/ruscv-sim"
LABEL org.opencontainers.image.description="Development and verification environment for ruscv-sim"
LABEL org.opencontainers.image.licenses="MIT"

CMD ["bash"]
