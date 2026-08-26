# Development Environment

**Status:** Current

**Authority:** Normative for the repository development container

**Last reviewed:** 2026-08-26

The root [`Dockerfile`](../Dockerfile) defines the reproducible development and
verification environment for `ruscv-sim`. It replaces the unversioned external
image referenced by archived milestone plans.

## Included tools

- Rust toolchain with Cargo, `rustfmt`, Clippy, and LLVM tools
- `cargo-llvm-cov`
- Native C/C++ build tools with their standard libraries, CMake, Ninja, Python,
  and GDB
- GNU `riscv64-unknown-elf` C, C++, assembler, linker, and binutils tools for
  freestanding bare-metal guests
- Spike reference ISS

The image is sufficient for the current Rust quality gate, project-authored
assembly guests, freestanding C/C++ guests, and Spike-based differential work.
The cross toolchain does not include a target libc, libstdc++, or startup
runtime: a guest using those facilities must supply a reviewed runtime and
linker setup. Native host-side C++ has its normal standard library, which is
needed by tools and future C++ test harnesses. SystemC is not part of the
baseline image because the current crate has no native SystemC build dependency.
Add it through a reviewed image change when the SystemC adapter becomes an
active integration target.

## Build

```bash
docker build --tag ruscv-sim-dev .
```

The defaults are versioned in the Dockerfile. They can be overridden explicitly
without editing the file:

```bash
docker build \
  --build-arg RUST_VERSION=1.97.1 \
  --build-arg SPIKE_VERSION=v1.1.0 \
  --build-arg DEV_UID="$(id -u)" \
  --build-arg DEV_GID="$(id -g)" \
  --tag ruscv-sim-dev .
```

Do not publish an overridden image as the project baseline without updating and
reviewing the defaults in the Dockerfile.

## Use interactively

```bash
docker run --rm -it --init \
  --volume "$PWD:/workspace" \
  --workdir /workspace \
  ruscv-sim-dev
```

The default container user has UID/GID 1000. Build with `DEV_UID` and `DEV_GID`
matching the host when necessary so files created in the bind mount retain the
expected ownership. Numeric IDs already used by a Debian system account are
accepted, which covers common macOS IDs such as GID 20.

## Published image

When a Docker definition change reaches `main`, the development-container
workflow runs the full Rust quality gate and project guest ELF suite before
publishing a multi-platform OCI image to:

```text
ghcr.io/mimiqdev/ruscv-sim-dev
```

It publishes two tags:

- `main` is the rolling, verified development baseline.
- `sha-<full-git-commit>` identifies the exact repository revision that
  produced the image.

After a successful publication, the workflow keeps the two newest container
versions carrying a `sha-*` tag and deletes older versions only when every tag
on that version is a `sha-*` tag. Any version also carrying `main`, `stable`, a
release tag, or another non-SHA tag is protected as a whole because GHCR deletes
package versions rather than individual tags. A deletion or package API failure
fails the workflow instead of silently bypassing retention.

Pull the rolling baseline with:

```bash
docker pull ghcr.io/mimiqdev/ruscv-sim-dev:main
```

The published OCI index contains native `linux/amd64` and `linux/arm64` images.
Docker Desktop on Apple Silicon therefore selects the ARM64 image without x86
emulation. Local builds likewise use the host architecture by default.

GHCR package visibility is managed in the repository/package settings. If the
package is private, authenticate with `docker login ghcr.io` before pulling.

Pull requests build the native CI image and run the same verification suite but
never authenticate to GHCR or publish a package. A manual workflow run publishes
only when it runs against the `main` branch. The ARM64 variant is rebuilt under
QEMU as part of the multi-platform publication; its Dockerfile toolchain smoke
tests run during that build. Publishing uses the workflow-scoped `GITHUB_TOKEN`;
no long-lived personal access token is required.

## Run the quality gate

```bash
docker run --rm --init \
  --volume "$PWD:/workspace" \
  --workdir /workspace \
  ruscv-sim-dev \
  bash -c 'cargo fmt --all -- --check && \
    cargo check --all-features && \
    cargo clippy --all-features --all-targets -- -D warnings && \
    cargo test --all-features && \
    cargo doc --all-features --no-deps'
```

Build and run the project-authored guest ELF set with:

```bash
docker run --rm --init \
  --volume "$PWD:/workspace" \
  --workdir /workspace \
  ruscv-sim-dev \
  bash -c './scripts/compile_riscv_tests.sh && ./scripts/run_elf_tests.sh'
```

## Version policy

- Rust, Spike, and Cargo-installed tools have explicit defaults in the
  Dockerfile.
- Debian packages are resolved from the selected Debian release when the image
  is rebuilt. Published project images must additionally be identified by an
  immutable image digest.
- `latest` is not an accepted normative development-image reference.
- A Dockerfile change must pass the container build workflow before its image is
  published or treated as the repository baseline.
