#!/bin/sh
set -eu

REPO="measure-sh/holo"
INSTALL_DIR="$HOME/.local/bin"

main() {
    need_cmd curl
    need_cmd tar

    os=$(uname -s)
    arch=$(uname -m)

    case "$os" in
        Darwin) os_target="apple-darwin" ;;
        Linux)  os_target="unknown-linux-gnu" ;;
        *)      err "unsupported OS: $os" ;;
    esac

    case "$arch" in
        x86_64)        arch_target="x86_64" ;;
        arm64|aarch64) arch_target="aarch64" ;;
        *)             err "unsupported architecture: $arch" ;;
    esac

    target="${arch_target}-${os_target}"

    printf "Fetching latest release... "
    tag=$(curl -sSL "https://api.github.com/repos/${REPO}/releases/latest" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p')
    if [ -z "$tag" ]; then
        err "failed to fetch latest release tag"
    fi
    printf "%s\n" "$tag"

    url="https://github.com/${REPO}/releases/download/${tag}/holo-${target}.tar.gz"

    printf "Downloading holo %s for %s... " "$tag" "$target"
    tmpdir=$(mktemp -d)
    trap 'rm -rf "$tmpdir"' EXIT
    if ! curl -sSL -o "$tmpdir/holo.tar.gz" "$url"; then
        err "failed to download $url"
    fi
    printf "done\n"

    tar xzf "$tmpdir/holo.tar.gz" -C "$tmpdir"

    mkdir -p "$INSTALL_DIR"
    mv "$tmpdir/holo" "$INSTALL_DIR/holo"
    chmod +x "$INSTALL_DIR/holo"

    printf "Installed holo to %s/holo\n" "$INSTALL_DIR"

    case ":${PATH}:" in
        *":${INSTALL_DIR}:"*) ;;
        *) printf "\nNote: %s is not in your PATH. Add it with:\n  export PATH=\"%s:\$PATH\"\n" "$INSTALL_DIR" "$INSTALL_DIR" ;;
    esac
}

need_cmd() {
    if ! command -v "$1" > /dev/null 2>&1; then
        err "required command not found: $1"
    fi
}

err() {
    printf "error: %s\n" "$1" >&2
    exit 1
}

main
