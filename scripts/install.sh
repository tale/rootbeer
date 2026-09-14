#!/bin/sh
# Rootbeer nightly installer
# Usage: sh -c "$(curl -fsSL rootbeer.tale.me/rb.sh)" -- init tale/dotfiles
set -e

BASE_URL="https://rootbeer.tale.me/nightly"
INSTALL_DIR="${HOME}/.rootbeer/bin"

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

main() {
	for tool in curl unzip; do
		if ! command -v "$tool" >/dev/null 2>&1; then
			echo "error: $tool is required; install it with your system package manager and retry" >&2
			exit 1
		fi
	done

	platform=$(detect_platform)
	if [ "$platform" = "macos-x86_64" ]; then
		echo "Intel macOS is unsupported. Installing the frozen final binary; no future Rootbeer or catalog updates are available." >&2
	fi

	artifact="rb-${platform}.zip"
	url="${BASE_URL}/${artifact}"

	tmpdir=$(mktemp -d)
	trap 'rm -rf "$tmpdir"' EXIT

	echo "downloading rootbeer nightly for ${platform}..."
	curl -fsSL "$url" -o "${tmpdir}/rb.zip"
	unzip -q "${tmpdir}/rb.zip" -d "${tmpdir}"
	chmod +x "${tmpdir}/rb"

	mkdir -p "${INSTALL_DIR}"
	mv "${tmpdir}/rb" "${INSTALL_DIR}/rb"

	echo ""
	echo "rootbeer installed to ${INSTALL_DIR}/rb"
	echo ""
	echo "add this to your shell profile:"
	echo "  export PATH=\"${INSTALL_DIR}:\$PATH\""

	if [ $# -gt 0 ]; then
		echo ""
		echo "running: ${INSTALL_DIR}/rb $*"
		PATH="${INSTALL_DIR}:$PATH" "${INSTALL_DIR}/rb" "$@"
	fi
}

main "$@"
