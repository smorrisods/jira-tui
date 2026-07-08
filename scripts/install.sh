#!/bin/sh
# jira-tui installer — downloads, verifies, and installs a release build.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/smorrisods/jira-tui/main/scripts/install.sh | sh
#   ./install.sh [--version vX.Y.Z] [--prefix /path] [--uninstall]
#
# Env vars (same effect as the matching flag): VERSION, PREFIX, NO_COLOR.
#
# Written in POSIX sh (not bash) so it works the same whether it's run
# directly or piped into `sh` from curl, on both Linux and macOS.

set -eu

REPO="smorrisods/jira-tui"
BINARY_NAME="jira-tui"

VERSION="${VERSION:-}"
PREFIX="${PREFIX:-/usr/local}"
UNINSTALL=false

usage() {
	cat <<'USAGE'
Usage: install.sh [options]

Options:
  --version <tag>   Install a specific release tag (default: latest)
  --prefix <path>   Install prefix (default: /usr/local, or $PREFIX)
  --uninstall       Remove a previously installed jira-tui and exit
  -h, --help        Show this help

Environment variables VERSION and PREFIX are equivalent to the matching
flags. Set NO_COLOR=1 to disable coloured output.
USAGE
}

while [ $# -gt 0 ]; do
	case "$1" in
		--version)
			VERSION="$2"
			shift 2
			;;
		--prefix)
			PREFIX="$2"
			shift 2
			;;
		--uninstall)
			UNINSTALL=true
			shift
			;;
		-h | --help)
			usage
			exit 0
			;;
		*)
			echo "Unknown option: $1" >&2
			usage >&2
			exit 1
			;;
	esac
done

# ── Colour + a bit of Jax 🦦 ──────────────────────────────────────────────────
if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
	BOLD="$(printf '\033[1m')"
	CYAN="$(printf '\033[36m')"
	GREEN="$(printf '\033[32m')"
	YELLOW="$(printf '\033[33m')"
	RED="$(printf '\033[31m')"
	RESET="$(printf '\033[0m')"
else
	BOLD="" CYAN="" GREEN="" YELLOW="" RED="" RESET=""
fi

info() { printf '%s\n' "${CYAN}${1}${RESET}"; }
success() { printf '%s\n' "${GREEN}${1}${RESET}"; }
warn() { printf '%s\n' "${YELLOW}${1}${RESET}" >&2; }
fail() {
	printf '%s\n' "${RED}${1}${RESET}" >&2
	exit 1
}

jax_banner() {
	printf '%s\n' "${CYAN}${BOLD}  🦦  jira-tui installer${RESET}"
}

jax_done() {
	printf '%s\n' "${GREEN}  🦦  Jax approves. Run '${BOLD}jira-tui${RESET}${GREEN}' to get started.${RESET}"
}

# ── OS / architecture detection ──────────────────────────────────────────────
detect_os() {
	case "$(uname -s)" in
		Linux) echo "linux" ;;
		Darwin) echo "macos" ;;
		*)
			fail "Unsupported OS: $(uname -s). jira-tui ships Linux and macOS releases only."
			;;
	esac
}

detect_arch() {
	case "$(uname -m)" in
		x86_64 | amd64) echo "amd64" ;;
		arm64 | aarch64) echo "arm64" ;;
		*)
			fail "Unsupported architecture: $(uname -m)."
			;;
	esac
}

sha256_verify() {
	# Verify $1 (a file) against the matching line in $2 (a SHA256SUMS file),
	# using whichever checksum tool is available on this host.
	file="$1"
	sums_file="$2"

	if command -v sha256sum >/dev/null 2>&1; then
		grep " $(basename "${file}")\$" "${sums_file}" | sha256sum -c - >/dev/null
		return
	fi

	if command -v shasum >/dev/null 2>&1; then
		grep " $(basename "${file}")\$" "${sums_file}" | shasum -a 256 -c - >/dev/null
		return
	fi

	fail "Need sha256sum or shasum to verify the download; neither was found."
}

download() {
	url="$1"
	output="$2"

	if command -v curl >/dev/null 2>&1; then
		curl -fsSL "${url}" -o "${output}"
	elif command -v wget >/dev/null 2>&1; then
		wget -q "${url}" -O "${output}"
	else
		fail "Need curl or wget to download the release."
	fi
}

# Whether `sudo` is needed to write into `$1` — walks up to the nearest
# existing ancestor directory to test writability, so a not-yet-created
# custom --prefix under a writable parent doesn't trigger an unnecessary
# sudo prompt.
need_sudo_for() {
	check_path="$1"
	while [ ! -e "${check_path}" ]; do
		check_path="$(dirname "${check_path}")"
	done
	if [ -w "${check_path}" ] || [ "$(id -u)" -eq 0 ]; then
		echo ""
	else
		echo "sudo"
	fi
}

uninstall() {
	jax_banner
	info "Removing jira-tui from ${PREFIX}..."

	uninstall_sudo="$(need_sudo_for "${PREFIX}")"
	if [ -n "${uninstall_sudo}" ]; then
		warn "${PREFIX} isn't writable by the current user — will use sudo to uninstall."
	fi

	removed=false
	if [ -f "${PREFIX}/bin/${BINARY_NAME}" ]; then
		${uninstall_sudo} rm -f "${PREFIX}/bin/${BINARY_NAME}"
		removed=true
	fi
	if [ -f "${PREFIX}/share/man/man1/${BINARY_NAME}.1.gz" ]; then
		${uninstall_sudo} rm -f "${PREFIX}/share/man/man1/${BINARY_NAME}.1.gz"
		removed=true
	fi

	if [ "${removed}" = true ]; then
		success "Uninstalled jira-tui from ${PREFIX}."
	else
		warn "No jira-tui install found under ${PREFIX} — nothing to do."
	fi
	exit 0
}

if [ "${UNINSTALL}" = true ]; then
	uninstall
fi

jax_banner

OS="$(detect_os)"
ARCH="$(detect_arch)"
PLATFORM="${OS}-${ARCH}"
info "Detected platform: ${PLATFORM}"

if [ -z "${VERSION}" ]; then
	info "Resolving the latest release..."
	VERSION="$(
		download "https://api.github.com/repos/${REPO}/releases/latest" /dev/stdout 2>/dev/null \
			| grep '"tag_name"' \
			| head -n 1 \
			| sed -E 's/.*"tag_name": *"([^"]+)".*/\1/'
	)"
	if [ -z "${VERSION}" ]; then
		fail "Could not resolve the latest release tag. Pass --version vX.Y.Z explicitly."
	fi
fi
info "Installing jira-tui ${VERSION} for ${PLATFORM} into ${PREFIX}"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

ASSET="${BINARY_NAME}-${VERSION}-${PLATFORM}.tar.gz"
ASSET_URL="https://github.com/${REPO}/releases/download/${VERSION}/${ASSET}"
SUMS_URL="https://github.com/${REPO}/releases/download/${VERSION}/SHA256SUMS"

info "Downloading ${ASSET}..."
download "${ASSET_URL}" "${TMP_DIR}/${ASSET}" \
	|| fail "Could not download ${ASSET_URL} — check that ${VERSION} has a ${PLATFORM} release."
download "${SUMS_URL}" "${TMP_DIR}/SHA256SUMS" \
	|| fail "Could not download SHA256SUMS for ${VERSION}."

info "Verifying checksum..."
(cd "${TMP_DIR}" && sha256_verify "${ASSET}" "SHA256SUMS") \
	|| fail "Checksum verification failed for ${ASSET}. Not installing."
success "Checksum OK."

info "Extracting..."
mkdir -p "${TMP_DIR}/extracted"
tar -xzf "${TMP_DIR}/${ASSET}" -C "${TMP_DIR}/extracted"

NEED_SUDO="$(need_sudo_for "${PREFIX}")"
if [ -n "${NEED_SUDO}" ]; then
	warn "${PREFIX} isn't writable by the current user — will use sudo to install."
fi

${NEED_SUDO} mkdir -p "${PREFIX}/bin" "${PREFIX}/share/man/man1"
${NEED_SUDO} install -m 0755 "${TMP_DIR}/extracted/bin/${BINARY_NAME}" "${PREFIX}/bin/${BINARY_NAME}"
${NEED_SUDO} install -m 0644 "${TMP_DIR}/extracted/share/man/man1/${BINARY_NAME}.1.gz" "${PREFIX}/share/man/man1/${BINARY_NAME}.1.gz"

success "Installed ${PREFIX}/bin/${BINARY_NAME} and its man page."

if [ "${OS}" = "macos" ]; then
	warn "macOS builds are ad-hoc signed only (no Apple notarization yet)."
	warn "First run may be blocked by Gatekeeper. If so, either right-click"
	warn "${BINARY_NAME} in Finder and choose Open, or run:"
	warn "  xattr -d com.apple.quarantine ${PREFIX}/bin/${BINARY_NAME}"
fi

case ":${PATH}:" in
	*":${PREFIX}/bin:"*) ;;
	*) warn "${PREFIX}/bin isn't on your \$PATH — add it to run '${BINARY_NAME}' directly." ;;
esac

jax_done
