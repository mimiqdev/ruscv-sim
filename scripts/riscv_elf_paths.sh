#!/bin/bash
# Shared path validation for the project-authored RISC-V ELF scripts.
#
# This file is sourced by the build/run scripts and by their guard tests.  It
# intentionally permits only one disposable directory directly below the
# canonical repository target directory.  It must not be executed as a
# standalone command.

riscv_validate_output_dir() {
    local project_dir="$1"
    local requested="$2"
    local target_root="${project_dir}/target"
    local candidate parent base canonical_parent canonical_target

    if [ -z "${project_dir}" ] || [ -z "${requested}" ]; then
        echo "[FAIL] project and output paths are required" >&2
        return 2
    fi

    # The caller already resolves PROJECT_DIR with pwd -P.  Refuse a symlinked
    # target root rather than allowing cleanup through an unexpected mount.
    if [ -L "${target_root}" ]; then
        echo "[FAIL] target root is a symlink: ${target_root}" >&2
        return 2
    fi
    if [ -e "${target_root}" ] && [ ! -d "${target_root}" ]; then
        echo "[FAIL] target root is not a directory: ${target_root}" >&2
        return 2
    fi
    if [ ! -e "${target_root}" ] && ! mkdir -p -- "${target_root}"; then
        echo "[FAIL] cannot create target root: ${target_root}" >&2
        return 2
    fi
    canonical_target="$(cd -- "${target_root}" 2>/dev/null && pwd -P)" || {
        echo "[FAIL] cannot canonicalize target root: ${target_root}" >&2
        return 2
    }

    # Relative values are repository-relative, not caller-working-directory
    # relative.  This makes the documented target/foo form deterministic.
    case "${requested}" in
        /*) candidate="${requested}" ;;
        *) candidate="${project_dir}/${requested}" ;;
    esac

    # Strip trailing separators before examining the final component.  A
    # repository path such as /repo/ is therefore still rejected below rather
    # than bypassing an exact-string guard.
    while [ "${candidate}" != "/" ] && [[ "${candidate}" == */ ]]; do
        candidate="${candidate%/}"
    done

    base="${candidate##*/}"
    if [ -z "${base}" ] || [ "${base}" = "." ] || [ "${base}" = ".." ]; then
        echo "[FAIL] output must be a named disposable directory: ${requested}" >&2
        return 2
    fi

    parent="${candidate%/*}"
    if [ "${parent}" = "${candidate}" ]; then
        parent="/"
    fi
    canonical_parent="$(cd -- "${parent}" 2>/dev/null && pwd -P)" || {
        echo "[FAIL] output parent does not exist: ${requested}" >&2
        return 2
    }

    # Require the lexical parent to be the canonical target path as well as
    # requiring its physical path to match.  The lexical check rejects ././/
    # aliases and any nested component that could hide a symlinked ancestor.
    if [ "${parent}" != "${canonical_target}" ] || [ "${canonical_parent}" != "${canonical_target}" ]; then
        echo "[FAIL] output must be directly below ${canonical_target}: ${requested}" >&2
        return 2
    fi

    # Never follow or remove an existing output symlink.  A regular file is
    # rejected too: the scripts only own disposable directories.
    if [ -L "${candidate}" ]; then
        echo "[FAIL] output path is a symlink: ${requested}" >&2
        return 2
    fi
    if [ -e "${candidate}" ] && [ ! -d "${candidate}" ]; then
        echo "[FAIL] output path is not a directory: ${requested}" >&2
        return 2
    fi

    # The parent is canonical target and the final component is not a symlink,
    # so this is the canonical path the caller may safely clean/create.
    printf '%s/%s\n' "${canonical_target}" "${base}"
}
