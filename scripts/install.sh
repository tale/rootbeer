#!/bin/sh
# Rootbeer nightly installer
# Usage: sh -c "$(curl -fsSL rbpkg.com/rb.sh)" -- init tale/dotfiles
set -e

BASE_URL="https://rbpkg.com/nightly"
PROFILE_BIN="${XDG_STATE_HOME:-${HOME}/.local/state}/rootbeer/profiles/user/current/bin"

detect_platform() {
	os=$(uname -s)
	arch=$(uname -m)

	case "$os" in
		Darwin) os="macos" ;;
		Linux)  os="linux" ;;
		*)
			echo "error: unsupported OS: $os" >&2
			exit 1
			;;
	esac

	case "$arch" in
		x86_64|amd64)  arch="x86_64" ;;
		arm64|aarch64) arch="aarch64" ;;
		*)
			echo "error: unsupported architecture: $arch" >&2
			exit 1
			;;
	esac

	echo "${os}-${arch}"
}

download() {
	if command -v curl >/dev/null 2>&1; then
		curl -fsSL "$1" -o "$2"
	elif command -v wget >/dev/null 2>&1; then
		wget -q "$1" -O "$2"
	else
		echo "error: curl or wget is required" >&2
		exit 1
	fi
}

main() {
	if ! command -v tar >/dev/null 2>&1; then
		echo "error: tar is required" >&2
		exit 1
	fi

	platform=$(detect_platform)
	if [ "$platform" = "macos-x86_64" ]; then
		echo "error: Intel macOS is unsupported; use Apple silicon macOS or Linux ARM64/x86-64." >&2
		exit 1
	fi

	tmpdir=$(mktemp -d)
	trap 'rm -rf "$tmpdir"' EXIT

	echo "downloading rootbeer nightly for ${platform}..."
	download "${BASE_URL}/rb-${platform}.tar.gz" "${tmpdir}/rb.tar.gz"
	tar -xzf "${tmpdir}/rb.tar.gz" -C "${tmpdir}"
	chmod +x "${tmpdir}/rb" "${tmpdir}/rb-store"

	# The downloaded rb is only a bootstrap; rootbeer installs and updates itself from the profile.
	"${tmpdir}/rb" use rootbeer

	echo ""
	echo "add this to your shell profile:"
	echo "  eval \"\$(\"${PROFILE_BIN}/rb\" env)\""

	if [ $# -gt 0 ]; then
		echo ""
		echo "running: rb $*"
		PATH="${PROFILE_BIN}:$PATH" "${PROFILE_BIN}/rb" "$@"
	fi
}

main "$@"
