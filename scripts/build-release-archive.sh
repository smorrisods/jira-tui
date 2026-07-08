#!/usr/bin/env bash
# Build a release tarball for the jira-tui binary and its clap_mangen-
# generated man page.

set -euo pipefail

VERSION=""
ARCH_INPUT=""
BINARY_PATH=""
MAN_DIR=""
OUTPUT_PREFIX=""

# Print CLI usage for local archive builds and workflow debugging.
usage() {
	cat <<'USAGE'
Usage: scripts/build-release-archive.sh [options]

Options:
  --version <version>         Release version or tag (for example, v0.1.0)
  --arch <amd64|arm64>        Target architecture
  --binary <path>             Built binary path
  --man-dir <path>            Directory containing the generated man page
  --output-prefix <prefix>    Output file prefix (without extension)
  -h, --help                  Show this help
USAGE
}

# Collapse common architecture aliases onto the two release architectures we support.
normalise_arch() {
	case "$1" in
		amd64 | x86_64)
			echo "amd64"
			;;
		arm64 | aarch64)
			echo "arm64"
			;;
		*)
			echo "Unsupported architecture: $1" >&2
			exit 1
			;;
	esac
}

# Find the newest clap_mangen-generated man directory next to the built release binary.
discover_man_dir() {
	local binary_path="$1"
	local release_dir
	release_dir="$(cd "$(dirname "${binary_path}")" && pwd)"

	# Cargo can leave multiple build-script outputs behind, so prefer the newest generated man dir.
	find "${release_dir}/build" -type d -path '*/out/man' -printf '%T@ %p\n' \
		| sort -nr \
		| head -n 1 \
		| cut -d' ' -f2-
}

while [[ $# -gt 0 ]]; do
	case "$1" in
		--version)
			VERSION="$2"
			shift 2
			;;
		--arch)
			ARCH_INPUT="$2"
			shift 2
			;;
		--binary)
			BINARY_PATH="$2"
			shift 2
			;;
		--man-dir)
			MAN_DIR="$2"
			shift 2
			;;
		--output-prefix)
			OUTPUT_PREFIX="$2"
			shift 2
			;;
		-h | --help)
			usage
			exit 0
			;;
		*)
			echo "Unknown option: $1" >&2
			usage
			exit 1
			;;
	esac
done

if [[ -z "${VERSION}" || -z "${ARCH_INPUT}" || -z "${BINARY_PATH}" || -z "${OUTPUT_PREFIX}" ]]; then
	echo "Missing required options." >&2
	usage
	exit 1
fi

ARCH="$(normalise_arch "${ARCH_INPUT}")"

if [[ ! -f "${BINARY_PATH}" ]]; then
	echo "Built binary not found at ${BINARY_PATH}" >&2
	exit 1
fi

if [[ -z "${MAN_DIR}" ]]; then
	MAN_DIR="$(discover_man_dir "${BINARY_PATH}")"
fi

if [[ -z "${MAN_DIR}" || ! -d "${MAN_DIR}" ]]; then
	echo "Generated man directory was not found." >&2
	exit 1
fi

if [[ ! -f "${MAN_DIR}/jira-tui.1" ]]; then
	echo "Generated man page jira-tui.1 was not found in ${MAN_DIR}" >&2
	exit 1
fi

mkdir -p "$(dirname "${OUTPUT_PREFIX}")"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

ARCHIVE_ROOT="${TMP_DIR}/jira-tui"
mkdir -p "${ARCHIVE_ROOT}/bin" "${ARCHIVE_ROOT}/share/man/man1"

# Tarballs unpack into a prefix-friendly layout so users can install into /usr/local or another prefix.
install -m 0755 "${BINARY_PATH}" "${ARCHIVE_ROOT}/bin/jira-tui"

# Match the package layout by shipping a compressed man page in share/man/man1.
gzip -n -c "${MAN_DIR}/jira-tui.1" > "${ARCHIVE_ROOT}/share/man/man1/jira-tui.1.gz"

ARCHIVE_OUTPUT="${OUTPUT_PREFIX}.tar.gz"
# Archive from the prepared prefix root so extraction lands in bin/ and share/man/man1/.
tar -C "${ARCHIVE_ROOT}" -czf "${ARCHIVE_OUTPUT}" .

echo "Built ${ARCHIVE_OUTPUT} for ${ARCH} from ${VERSION}"
