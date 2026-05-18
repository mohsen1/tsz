#!/usr/bin/env bash
# tsz installer — POSIX/macOS/Linux
# Usage:
#   curl -fsSL https://tsz.dev/install | bash
#   curl -fsSL https://tsz.dev/install | bash -s -- --version v0.1.9 --dir ~/bin
set -euo pipefail

REPO_OWNER="${TSZ_REPO_OWNER:-mohsen1}"
REPO_NAME="${TSZ_REPO_NAME:-tsz}"
VERSION="${TSZ_VERSION:-latest}"
INSTALL_DIR="${TSZ_INSTALL_DIR:-}"
BINS=("tsz" "tsz-lsp")

while [ $# -gt 0 ]; do
    case "$1" in
        --version) VERSION="$2"; shift 2 ;;
        --dir) INSTALL_DIR="$2"; shift 2 ;;
        --owner) REPO_OWNER="$2"; shift 2 ;;
        --repo) REPO_NAME="$2"; shift 2 ;;
        -h|--help)
            cat <<EOF
tsz installer
  --version <tag>   Release tag (default: latest)
  --dir <path>      Install directory (default: auto)
  --owner <owner>   Repo owner (default: mohsen1)
  --repo <name>     Repo name (default: tsz)
EOF
            exit 0 ;;
        *) echo "Unknown flag: $1" >&2; exit 2 ;;
    esac
done

say() { printf '\033[1;36m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m!!\033[0m %s\n' "$*" >&2; }
die() { printf '\033[1;31mxx\033[0m %s\n' "$*" >&2; exit 1; }

need() { command -v "$1" >/dev/null 2>&1 || die "missing dependency: $1"; }
need curl
need tar
need uname

pick_install_dir() {
    if [ -n "$INSTALL_DIR" ]; then
        mkdir -p "$INSTALL_DIR"
        echo "$INSTALL_DIR"
        return
    fi
    local candidates=("$HOME/.local/bin" "$HOME/bin" "/usr/local/bin" "/opt/homebrew/bin")
    for d in "${candidates[@]}"; do
        if [ "$d" = "$HOME/.local/bin" ] || [ "$d" = "$HOME/bin" ]; then
            mkdir -p "$d" 2>/dev/null || true
        fi
        if [ -d "$d" ] && [ -w "$d" ]; then
            echo "$d"
            return
        fi
    done
    mkdir -p "$HOME/.local/bin"
    echo "$HOME/.local/bin"
}

detect_target() {
    local os arch libc
    os="$(uname -s)"
    arch="$(uname -m)"
    case "$os" in
        Linux)
            if ldd --version 2>&1 | grep -qi musl; then
                libc="musl"
            else
                libc="gnu"
            fi
            case "$arch" in
                x86_64|amd64) echo "x86_64-unknown-linux-$libc" ;;
                aarch64|arm64) echo "aarch64-unknown-linux-$libc" ;;
                *) die "unsupported linux arch: $arch" ;;
            esac
            ;;
        Darwin)
            case "$arch" in
                x86_64) echo "x86_64-apple-darwin" ;;
                arm64) echo "aarch64-apple-darwin" ;;
                *) die "unsupported darwin arch: $arch" ;;
            esac
            ;;
        *) die "unsupported OS: $os (use install.ps1 for Windows)" ;;
    esac
}

resolve_version() {
    if [ "$VERSION" = "latest" ]; then
        echo "latest"
    else
        echo "$VERSION"
    fi
}

resolve_github_latest() {
    local tag
    tag=$(curl -fsSL "https://api.github.com/repos/${REPO_OWNER}/${REPO_NAME}/releases/latest" 2>/dev/null \
        | grep -E '"tag_name"' | head -n1 | sed -E 's/.*"tag_name"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/')
    [ -n "$tag" ] || die "could not determine latest versioned release tag for ${REPO_OWNER}/${REPO_NAME}"
    echo "$tag"
}

download_asset() {
    local tag="$1"
    local asset="$2"
    local url="https://github.com/${REPO_OWNER}/${REPO_NAME}/releases/download/${tag}/${asset}"

    say "downloading from $url"
    curl -fL --retry 3 --connect-timeout 10 -o "$TMP/$asset" "$url"
}

resolve_download() {
    if [ "$VERSION" = "latest" ]; then
        if download_asset "latest" "$ASSET"; then
            return
        fi

        if [ "${TSZ_INSTALL_REQUIRE_LATEST_CHANNEL:-0}" = "1" ]; then
            die "latest channel asset is not available for ${TARGET}"
        fi

        warn "latest channel asset is not available for ${TARGET}; falling back to the latest versioned release"
        TAG="$(resolve_github_latest)"
        ASSET="tsz-${TAG}-${TARGET}.tar.gz"
        download_asset "$TAG" "$ASSET" || die "download failed — does ${TAG} have a build for ${TARGET}?"
        return
    fi

    download_asset "$TAG" "$ASSET" || die "download failed — does ${TAG} have a build for ${TARGET}?"
}

TARGET="$(detect_target)"
TAG="$(resolve_version)"
INSTALL_DIR="$(pick_install_dir)"
ASSET="tsz-${TAG}-${TARGET}.tar.gz"

say "version:       $TAG"
say "target:        $TARGET"
say "asset:         $ASSET"
say "install dir:   $INSTALL_DIR"

TMP="$(mktemp -d -t tsz-install-XXXXXX)"
trap 'rm -rf "$TMP"' EXIT

resolve_download

say "extracting"
tar -xzf "$TMP/$ASSET" -C "$TMP"

# Layout in tarball: tsz-${TAG}-${TARGET}/<binary>
INNER="$TMP/tsz-${TAG}-${TARGET}"
[ -d "$INNER" ] || INNER="$TMP/tsz-${TARGET}"
[ -d "$INNER" ] || die "unexpected tarball layout; contents: $(ls "$TMP")"

for bin in "${BINS[@]}"; do
    src="$INNER/$bin"
    if [ -f "$src" ]; then
        install -m 0755 "$src" "$INSTALL_DIR/$bin"
        say "installed $INSTALL_DIR/$bin"
    fi
done

if ! command -v tsz >/dev/null 2>&1 || [ "$(command -v tsz)" != "$INSTALL_DIR/tsz" ]; then
    case ":$PATH:" in
        *":$INSTALL_DIR:"*) : ;;
        *)
            warn "$INSTALL_DIR is not on your PATH"
            warn "add this to your shell rc:  export PATH=\"$INSTALL_DIR:\$PATH\""
            ;;
    esac
fi

say "done — try: tsz --version"
