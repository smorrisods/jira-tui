#!/usr/bin/env bash
# Build Linux release packages (.deb and .rpm) for the jira-tui binary and
# its clap_mangen-generated man page.

set -euo pipefail

VERSION=""
ARCH_INPUT=""
BINARY_PATH=""
MAN_DIR=""
OUTPUT_PREFIX=""
FORMAT="all"

PACKAGE_NAME="jira-tui"
PACKAGE_SUMMARY="A developer-first, keyboard-driven Jira terminal UI"
PACKAGE_DESCRIPTION="jira-tui is a keyboard-driven Jira terminal UI with ADF-native rendering, a fully explorable offline demo mode, and optional live Jira sync."
PACKAGE_HOMEPAGE="https://github.com/smorrisods/jira-tui"
PACKAGE_VENDOR="Liminal HQ"
PACKAGE_CONTACT="Liminal HQ <contact@liminalhq.ca>"

# Print CLI usage for local packaging runs and workflow debugging.
usage() {
	cat <<'USAGE'
Usage: scripts/build-linux-packages.sh [options]

Options:
  --version <version>         Package version or tag (for example, v0.1.0)
  --arch <amd64|arm64>        Target architecture
  --binary <path>             Built binary path
  --man-dir <path>            Directory containing the generated man page
  --output-prefix <prefix>    Output file prefix (without extension)
  --format <all|deb|rpm>      Package format to build (default: all)
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
# Modification time of $1, in epoch seconds. BSD `stat` (macOS) and GNU
# `stat` (Linux) use incompatible flags for the same thing (`-f FORMAT` on
# BSD vs `-c FORMAT` on GNU) -- and critically, GNU's `-f` means something
# else entirely ("show filesystem status"), so a naive try-then-fallback
# doesn't fail cleanly on Linux and silently mixes garbage output into the
# result. Branch on the actual OS instead.
dir_mtime() {
	if [[ "$(uname -s)" == "Darwin" ]]; then
		stat -f '%m' "$1"
	else
		stat -c '%Y' "$1"
	fi
}

discover_man_dir() {
	local binary_path="$1"
	local release_dir
	release_dir="$(cd "$(dirname "${binary_path}")" && pwd)"

	# Cargo can leave multiple build-script outputs behind, so prefer the
	# newest generated man dir. Uses `dir_mtime` (not GNU-only `find
	# -printf`) so this stays portable on macOS's BSD find/stat too.
	local dir mtime
	find "${release_dir}/build" -type d -path '*/out/man' 2>/dev/null | while IFS= read -r dir; do
		mtime="$(dir_mtime "${dir}")"
		printf '%s %s\n' "${mtime}" "${dir}"
	done | sort -rn | head -n 1 | cut -d' ' -f2-
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
		--format)
			FORMAT="$2"
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

case "${FORMAT}" in
	all | deb | rpm)
		;;
	*)
		echo "Unsupported format: ${FORMAT}" >&2
		exit 1
		;;
esac

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

VERSION_NO_V="${VERSION#v}"
mkdir -p "$(dirname "${OUTPUT_PREFIX}")"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

MAN_SOURCE_DIR="${TMP_DIR}/man"
mkdir -p "${MAN_SOURCE_DIR}"

# Package installs expect a compressed man page, so gzip it once up front for both formats.
gzip -n -c "${MAN_DIR}/jira-tui.1" > "${MAN_SOURCE_DIR}/jira-tui.1.gz"

DEB_ARCH=""
RPM_ARCH=""
RPM_TARGET=""
case "${ARCH}" in
	amd64)
		DEB_ARCH="amd64"
		RPM_ARCH="x86_64"
		RPM_TARGET="x86_64-linux"
		;;
	arm64)
		DEB_ARCH="arm64"
		RPM_ARCH="aarch64"
		RPM_TARGET="aarch64-linux"
		;;
esac

build_deb() {
	local deb_root="${TMP_DIR}/deb-root"
	mkdir -p "${deb_root}/DEBIAN" "${deb_root}/usr/bin" "${deb_root}/usr/share/man/man1"

	# Debian packages install into distro-managed paths rather than a user-managed prefix.
	install -m 0755 "${BINARY_PATH}" "${deb_root}/usr/bin/jira-tui"
	install -m 0644 "${MAN_SOURCE_DIR}/jira-tui.1.gz" "${deb_root}/usr/share/man/man1/"

	cat > "${deb_root}/DEBIAN/control" <<CONTROL
Package: ${PACKAGE_NAME}
Version: ${VERSION_NO_V}
Section: utils
Priority: optional
Architecture: ${DEB_ARCH}
Maintainer: ${PACKAGE_CONTACT}
Homepage: ${PACKAGE_HOMEPAGE}
Depends: libc6
Description: ${PACKAGE_SUMMARY}
 ${PACKAGE_DESCRIPTION}
CONTROL

	cat > "${deb_root}/DEBIAN/postinst" <<'POSTINST'
#!/bin/sh
set -e
if command -v mandb >/dev/null 2>&1; then
	mandb -q >/dev/null 2>&1 || true
fi
POSTINST

	cat > "${deb_root}/DEBIAN/postrm" <<'POSTRM'
#!/bin/sh
set -e
if command -v mandb >/dev/null 2>&1; then
	mandb -q >/dev/null 2>&1 || true
fi
POSTRM

	chmod 0755 "${deb_root}/DEBIAN/postinst" "${deb_root}/DEBIAN/postrm"

	local deb_output="${OUTPUT_PREFIX}.deb"
	dpkg-deb --build "${deb_root}" "${deb_output}"
	echo "Built ${deb_output}"
}

# Build an RPM using a temporary rpmbuild root so release jobs stay self-contained.
build_rpm() {
	if ! command -v rpmbuild >/dev/null 2>&1; then
		echo "rpmbuild is required to create RPM packages" >&2
		exit 1
	fi

	local rpm_root="${TMP_DIR}/rpm"
	mkdir -p "${rpm_root}/BUILD" "${rpm_root}/BUILDROOT" "${rpm_root}/RPMS" "${rpm_root}/SOURCES" "${rpm_root}/SPECS" "${rpm_root}/SRPMS"

	install -m 0755 "${BINARY_PATH}" "${rpm_root}/SOURCES/jira-tui"
	install -m 0644 "${MAN_SOURCE_DIR}/jira-tui.1.gz" "${rpm_root}/SOURCES/"

	cat > "${rpm_root}/SPECS/jira-tui.spec" <<SPEC
Name:           ${PACKAGE_NAME}
Version:        ${VERSION_NO_V}
Release:        1%{?dist}
Summary:        ${PACKAGE_SUMMARY}
License:        MIT
URL:            ${PACKAGE_HOMEPAGE}
Vendor:         ${PACKAGE_VENDOR}
Packager:       ${PACKAGE_CONTACT}
BuildArch:      ${RPM_ARCH}

%description
${PACKAGE_DESCRIPTION}

%install
mkdir -p %{buildroot}/usr/bin %{buildroot}/usr/share/man/man1
install -m 0755 %{_sourcedir}/jira-tui %{buildroot}/usr/bin/jira-tui
install -m 0644 %{_sourcedir}/jira-tui.1.gz %{buildroot}/usr/share/man/man1/

%post
if command -v mandb >/dev/null 2>&1; then
	mandb -q >/dev/null 2>&1 || true
fi

%postun
if command -v mandb >/dev/null 2>&1; then
	mandb -q >/dev/null 2>&1 || true
fi

%files
/usr/bin/jira-tui
/usr/share/man/man1/jira-tui.1.gz

%changelog
* $(date '+%a %b %d %Y') Liminal HQ <contact@liminalhq.ca> - ${VERSION_NO_V}-1
- Add Linux RPM package for the jira-tui binary and man page.
SPEC

	rpmbuild \
		--define "_topdir ${rpm_root}" \
		--define "__os_install_post %{nil}" \
		--target "${RPM_TARGET}" \
		-bb "${rpm_root}/SPECS/jira-tui.spec"

	local rpm_built
	rpm_built="$(find "${rpm_root}/RPMS" -type f -name '*.rpm' | head -n 1)"
	if [[ -z "${rpm_built}" ]]; then
		echo "RPM build succeeded but no RPM file was produced" >&2
		exit 1
	fi

	local rpm_output="${OUTPUT_PREFIX}.rpm"
	cp "${rpm_built}" "${rpm_output}"
	echo "Built ${rpm_output}"
}

if [[ "${FORMAT}" == "all" || "${FORMAT}" == "deb" ]]; then
	build_deb
fi

if [[ "${FORMAT}" == "all" || "${FORMAT}" == "rpm" ]]; then
	build_rpm
fi
