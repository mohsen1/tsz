#!/bin/bash
# Materialize the exact pinned microsoft/TypeScript test corpus.
#
# Despite the historical filename, TypeScript/ is a standalone checkout, not a
# tracked git submodule. Keep this entrypoint for callers while avoiding gitlink
# state and the full-history fallback that used to fill developer disks.

set -euo pipefail

usage() {
    cat <<'EOF'
Usage:
  scripts/setup/reset-ts-submodule.sh [--sparse] [--force-reset]

Creates or resets the standalone TypeScript/ corpus checkout to the exact SHA
recorded in scripts/ci/typescript-submodule-ref. The clone/fetch is shallow and
blob-filtered. --sparse keeps only the corpus, libraries, and harness sources
needed by TSZ's local conformance, emit, and fourslash lanes.

By default, a dirty local corpus is left untouched. --force-reset explicitly
discards tracked edits and untracked files; ignored dependency/build caches are
still preserved. Shared symlink checkouts are never mutated by this helper.

For focused tests or an internal mirror, set TSZ_TYPESCRIPT_REPOSITORY to a
different microsoft/TypeScript-compatible Git URL.
EOF
}

SPARSE=false
FORCE_RESET=false
while [ $# -gt 0 ]; do
    case "$1" in
        --sparse)
            SPARSE=true
            shift
            ;;
        --force-reset)
            FORCE_RESET=true
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1 (try --help)" >&2
            exit 2
            ;;
    esac
done

# Git hooks can export repository-routing variables that override `git -C`.
# This helper must only ever operate on the standalone corpus checkout.
unset GIT_DIR GIT_INDEX_FILE GIT_WORK_TREE GIT_COMMON_DIR
unset GIT_OBJECT_DIRECTORY GIT_ALTERNATE_OBJECT_DIRECTORIES

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")"/../.. && pwd)"
REF_FILE="$ROOT_DIR/scripts/ci/typescript-submodule-ref"
VERSIONS_FILE="$ROOT_DIR/scripts/conformance/typescript-versions.json"
CORPUS_PATH="$ROOT_DIR/TypeScript"
CORPUS_REPOSITORY="${TSZ_TYPESCRIPT_REPOSITORY:-https://github.com/microsoft/TypeScript.git}"
REPOSITORY_OVERRIDE=false
if [ -n "${TSZ_TYPESCRIPT_REPOSITORY:-}" ]; then
    REPOSITORY_OVERRIDE=true
fi

if [ ! -f "$REF_FILE" ]; then
    echo "ERROR: Missing TypeScript corpus ref: $REF_FILE" >&2
    exit 1
fi
if [ ! -f "$VERSIONS_FILE" ]; then
    echo "ERROR: Missing TypeScript version map: $VERSIONS_FILE" >&2
    exit 1
fi

PINNED_SHA="$(tr -d '[:space:]' < "$REF_FILE")"
MAPPED_SHA="$(sed -n 's/^[[:space:]]*"current"[[:space:]]*:[[:space:]]*"\([0-9a-f][0-9a-f]*\)".*/\1/p' "$VERSIONS_FILE" | head -1)"
case "$PINNED_SHA" in
    ''|*[!0-9a-f]*)
        echo "ERROR: Invalid TypeScript corpus SHA in $REF_FILE: ${PINNED_SHA:-<empty>}" >&2
        exit 1
        ;;
esac
if [ "${#PINNED_SHA}" -ne 40 ]; then
    echo "ERROR: TypeScript corpus SHA must contain 40 lowercase hex characters: $PINNED_SHA" >&2
    exit 1
fi
if [ "$MAPPED_SHA" != "$PINNED_SHA" ]; then
    echo "ERROR: TypeScript corpus pins disagree:" >&2
    echo "  $REF_FILE: $PINNED_SHA" >&2
    echo "  $VERSIONS_FILE current: ${MAPPED_SHA:-<missing>}" >&2
    exit 1
fi

validate_checkout() {
    local actual_sha
    actual_sha="$(git -C "$CORPUS_PATH" rev-parse HEAD 2>/dev/null || true)"
    if [ "$actual_sha" != "$PINNED_SHA" ]; then
        echo "ERROR: TypeScript corpus is at ${actual_sha:-<no HEAD>}, expected $PINNED_SHA" >&2
        return 1
    fi
    for required in tests/cases tests/lib src/lib/es5.d.ts; do
        if [ ! -e "$CORPUS_PATH/$required" ]; then
            echo "ERROR: Pinned TypeScript corpus is missing $required" >&2
            return 1
        fi
    done
}

configure_checkout_shape() {
    if [ "$SPARSE" = true ]; then
        git -C "$CORPUS_PATH" sparse-checkout init --cone
        git -C "$CORPUS_PATH" sparse-checkout set \
            tests \
            src/lib \
            lib \
            src/compiler \
            src/services \
            src/harness \
            src/jsTyping \
            src/deprecatedCompat \
            src/server \
            src/executeCommandLine \
            src/typingsInstallerCore \
            src/cancellationToken \
            src/watchGuard \
            src/testRunner \
            scripts
    elif [ "$(git -C "$CORPUS_PATH" config --bool core.sparseCheckout 2>/dev/null || true)" = true ]; then
        git -C "$CORPUS_PATH" sparse-checkout disable
    fi
}

read_dirty_state() {
    git -C "$CORPUS_PATH" status --porcelain --untracked-files=normal
}

refuse_dirty_without_force() {
    local dirty_state="$1"
    if [ -n "$dirty_state" ] && [ "$FORCE_RESET" != true ]; then
        echo "ERROR: TypeScript corpus has local changes; refusing to discard them." >&2
        printf '%s\n' "$dirty_state" | sed -n '1,20p' >&2
        echo "       Re-run with --force-reset to discard tracked edits and untracked files." >&2
        return 1
    fi
}

migrate_legacy_module_gitdir() {
    [ -f "$CORPUS_PATH/.git" ] || return 0

    local actual_git_dir root_common_dir expected_git_dir
    actual_git_dir="$(git -C "$CORPUS_PATH" rev-parse --absolute-git-dir 2>/dev/null || true)"
    root_common_dir="$(git -C "$ROOT_DIR" rev-parse --git-common-dir 2>/dev/null || true)"
    if [ -z "$actual_git_dir" ] || [ -z "$root_common_dir" ]; then
        echo "ERROR: TypeScript/.git is a gitfile but its ownership cannot be verified; refusing to mutate it." >&2
        return 1
    fi
    case "$root_common_dir" in
        /*) ;;
        *) root_common_dir="$ROOT_DIR/$root_common_dir" ;;
    esac
    if [ ! -d "$actual_git_dir" ] || [ -L "$actual_git_dir" ] || [ ! -d "$root_common_dir" ]; then
        echo "ERROR: TypeScript/.git does not point at a safe legacy module gitdir; refusing to mutate it." >&2
        return 1
    fi
    actual_git_dir="$(cd "$actual_git_dir" && pwd -P)"
    root_common_dir="$(cd "$root_common_dir" && pwd -P)"
    expected_git_dir="$root_common_dir/modules/TypeScript"
    if [ ! -d "$expected_git_dir" ] || [ "$(cd "$expected_git_dir" && pwd -P)" != "$actual_git_dir" ]; then
        echo "ERROR: TypeScript/.git points outside the legacy module path; refusing to mutate it." >&2
        echo "       Git dir: $actual_git_dir" >&2
        return 1
    fi

    # A linked-worktree store has its own worktrees/ registry. The historical
    # submodule gitdir does not; its gitfile is the sole worktree association.
    if [ -d "$actual_git_dir/worktrees" ]; then
        echo "ERROR: Legacy TypeScript gitdir has linked worktrees; refusing to move it." >&2
        return 1
    fi

    local backup_link legacy_core_worktree legacy_worktree_core_worktree
    backup_link="$CORPUS_PATH/.git.legacy-link.$$"
    legacy_core_worktree="$(git config --file "$actual_git_dir/config" --get core.worktree 2>/dev/null || true)"
    legacy_worktree_core_worktree="$(git config --file "$actual_git_dir/config.worktree" --get core.worktree 2>/dev/null || true)"
    if [ -e "$backup_link" ]; then
        echo "ERROR: Legacy gitdir migration backup already exists: $backup_link" >&2
        return 1
    fi

    mv "$CORPUS_PATH/.git" "$backup_link"
    if ! mv "$actual_git_dir" "$CORPUS_PATH/.git"; then
        mv "$backup_link" "$CORPUS_PATH/.git"
        echo "ERROR: Could not move the legacy TypeScript gitdir into the corpus checkout." >&2
        return 1
    fi
    git config --file "$CORPUS_PATH/.git/config" --unset-all core.worktree >/dev/null 2>&1 || true
    if [ -f "$CORPUS_PATH/.git/config.worktree" ]; then
        git config --file "$CORPUS_PATH/.git/config.worktree" --unset-all core.worktree >/dev/null 2>&1 || true
    fi

    local migrated_git_dir migrated_top
    migrated_git_dir="$(git -C "$CORPUS_PATH" rev-parse --absolute-git-dir 2>/dev/null || true)"
    migrated_top="$(git -C "$CORPUS_PATH" rev-parse --show-toplevel 2>/dev/null || true)"
    if [ "$migrated_git_dir" != "$CORPUS_PATH/.git" ] \
        || [ "$(cd "$migrated_top" 2>/dev/null && pwd -P || true)" != "$(cd "$CORPUS_PATH" && pwd -P)" ]; then
        if [ -n "$legacy_core_worktree" ]; then
            git config --file "$CORPUS_PATH/.git/config" core.worktree "$legacy_core_worktree" || true
        fi
        if [ -n "$legacy_worktree_core_worktree" ]; then
            git config --file "$CORPUS_PATH/.git/config.worktree" core.worktree "$legacy_worktree_core_worktree" || true
        fi
        if mv "$CORPUS_PATH/.git" "$actual_git_dir" && mv "$backup_link" "$CORPUS_PATH/.git"; then
            echo "ERROR: Legacy TypeScript gitdir migration failed validation and was rolled back." >&2
        else
            echo "ERROR: Legacy TypeScript gitdir migration failed; manual recovery is required." >&2
        fi
        return 1
    fi

    rm "$backup_link"
    rmdir "$(dirname "$actual_git_dir")" 2>/dev/null || true
    echo "Migrated legacy TypeScript module gitdir into the standalone checkout."
}

# Shared worktree corpora are intentionally immutable from this checkout.
if [ -L "$CORPUS_PATH" ]; then
    validate_checkout || {
        echo "ERROR: TypeScript is a shared symlink; repair its source checkout instead." >&2
        exit 1
    }
    if ! SHARED_DIRTY_STATE="$(read_dirty_state 2>/dev/null)"; then
        echo "ERROR: Cannot inspect shared TypeScript corpus state; refusing to mutate it." >&2
        exit 1
    fi
    if [ -n "$SHARED_DIRTY_STATE" ]; then
        echo "ERROR: Shared TypeScript corpus checkout is dirty; refusing to mutate it." >&2
        echo "       Clean the source checkout before using this worktree." >&2
        exit 1
    fi
    echo "TypeScript corpus symlink verified at $PINNED_SHA"
    exit 0
fi

if { [ -d "$CORPUS_PATH/.git" ] || [ -f "$CORPUS_PATH/.git" ]; } \
    && git -C "$CORPUS_PATH" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    ACTUAL_SHA="$(git -C "$CORPUS_PATH" rev-parse HEAD 2>/dev/null || true)"
    if ! DIRTY_STATE="$(read_dirty_state 2>/dev/null)"; then
        echo "ERROR: Cannot inspect TypeScript corpus state; refusing to mutate it." >&2
        exit 1
    fi
    refuse_dirty_without_force "$DIRTY_STATE" || exit 1
    migrate_legacy_module_gitdir || exit 1
    if [ "$ACTUAL_SHA" = "$PINNED_SHA" ] && [ -z "$DIRTY_STATE" ]; then
        configure_checkout_shape
        validate_checkout
        echo "TypeScript corpus already pinned at $PINNED_SHA"
        exit 0
    fi
else
    if [ -e "$CORPUS_PATH" ]; then
        if [ -d "$CORPUS_PATH" ] && [ -z "$(ls -A "$CORPUS_PATH" 2>/dev/null)" ]; then
            rmdir "$CORPUS_PATH"
        else
            echo "ERROR: $CORPUS_PATH exists but is not a Git checkout; refusing to delete it." >&2
            exit 1
        fi
    fi

    echo "Cloning the pinned TypeScript corpus without repository history..."
    git clone --filter=blob:none --no-checkout --depth 1 \
        "$CORPUS_REPOSITORY" "$CORPUS_PATH"
fi

if git -C "$CORPUS_PATH" cat-file -e "$PINNED_SHA^{commit}" 2>/dev/null; then
    echo "Using locally available TypeScript corpus $PINNED_SHA."
else
    FETCH_SOURCE=origin
    if [ "$REPOSITORY_OVERRIDE" = true ]; then
        FETCH_SOURCE="$CORPUS_REPOSITORY"
    elif ! git -C "$CORPUS_PATH" remote get-url origin >/dev/null 2>&1; then
        git -C "$CORPUS_PATH" remote add origin "$CORPUS_REPOSITORY"
    fi
    echo "Fetching TypeScript corpus $PINNED_SHA..."
    git -C "$CORPUS_PATH" fetch --filter=blob:none --depth 1 "$FETCH_SOURCE" "$PINNED_SHA"
fi

if [ "$FORCE_RESET" = true ]; then
    git -C "$CORPUS_PATH" checkout --detach --force "$PINNED_SHA"
    git -C "$CORPUS_PATH" reset --hard --quiet "$PINNED_SHA"
    git -C "$CORPUS_PATH" clean -fd --quiet
else
    git -C "$CORPUS_PATH" checkout --detach "$PINNED_SHA"
fi
configure_checkout_shape

validate_checkout
if ! FINAL_DIRTY_STATE="$(read_dirty_state 2>/dev/null)" || [ -n "$FINAL_DIRTY_STATE" ]; then
    echo "ERROR: TypeScript corpus is not clean after reset." >&2
    exit 1
fi
echo "TypeScript corpus reset to $PINNED_SHA"
