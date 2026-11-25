#!/bin/sh
# Installs Xilo on macOS and Linux, for the current user.
#
#   curl -fsSL https://raw.githubusercontent.com/ftchd/xilo/main/install.sh | sh
#
# Options, as environment variables:
#   XILO_VERSION=v0.2.0   Install this release instead of the latest one.
#   XILO_NO_OPEN=1        Do not open Xilo after installing.
#   XILO_DRY_RUN=1        Show what the script would do, and change nothing.
#
# To uninstall. Your settings and history stay, and the script says where they are:
#   curl -fsSL https://raw.githubusercontent.com/ftchd/xilo/main/install.sh | sh -s -- --uninstall

set -eu

repo="ftchd/xilo"
releases="${XILO_DOWNLOAD_URL:-https://github.com/$repo/releases}"
version="${XILO_VERSION:-latest}"
# The id of the app bundle, and the label of the login item that Xilo writes.
bundle_id="dev.xilo.app"

say() { printf '%s\n' "$*"; }
fail() {
	printf 'xilo: %s\n' "$*" >&2
	exit 1
}
has() { command -v "$1" >/dev/null 2>&1; }

usage() {
	say "Usage: install.sh [--uninstall]"
	say ""
	say "Installs Xilo and the xilo command for the current user, then opens Xilo."
	say "Set XILO_VERSION to pick a release, XILO_NO_OPEN=1 to not open Xilo,"
	say "and XILO_DRY_RUN=1 to see what the script would do."
}

download() {
	if has curl; then
		curl -fsSL --retry 3 -o "$2" "$1"
	elif has wget; then
		wget -q -O "$2" "$1"
	else
		fail "install curl or wget, then try again."
	fi
}

sha256() {
	if has sha256sum; then
		sha256sum "$1" | cut -d ' ' -f 1
	elif has shasum; then
		shasum -a 256 "$1" | cut -d ' ' -f 1
	else
		fail "cannot check the download. Install sha256sum or shasum, then try again."
	fi
}

detect() {
	case "$(uname -s)" in
	Darwin) os=macos ;;
	Linux) os=linux ;;
	*) fail "this script is for macOS and Linux. On Windows, use install.ps1." ;;
	esac

	case "$(uname -m)" in
	x86_64 | amd64) arch=x64 ;;
	arm64 | aarch64) arch=arm64 ;;
	*) fail "there is no Xilo build for $(uname -m)." ;;
	esac

	# A shell that runs under Rosetta reports x86_64 on Apple silicon.
	if [ "$os" = macos ] && [ "$arch" = x64 ] && [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || true)" = 1 ]; then
		arch=arm64
	fi

	if [ "$os" = macos ]; then
		suffix="-macos-$arch.dmg"
	else
		suffix="-linux-$arch.tar.gz"
	fi

	case "$version" in
	latest) base="$releases/latest/download" ;;
	v*) base="$releases/download/$version" ;;
	*)
		version="v$version"
		base="$releases/download/$version"
		;;
	esac
}

# The file names have the version in them, so the script reads the name from
# SHA256SUMS.txt, downloads the file into $tmp and checks it.
fetch() {
	download "$base/SHA256SUMS.txt" "$tmp/SHA256SUMS.txt" || fail "could not download $base/SHA256SUMS.txt. Is there a release yet?"
	line=$(awk -v suffix="$suffix" '{
		name = $2
		sub(/^\*/, "", name)
		if (name ~ /^Xilo-/ && substr(name, length(name) - length(suffix) + 1) == suffix) { print $1, name; exit }
	}' "$tmp/SHA256SUMS.txt")
	[ -n "$line" ] || fail "the release has no file that ends with $suffix."
	expected=${line%% *}
	asset=${line#* }

	say "Downloading $asset..."
	download "$base/$asset" "$tmp/$asset" || fail "could not download $base/$asset."
	actual=$(sha256 "$tmp/$asset")
	[ "$actual" = "$expected" ] || fail "the download of $asset is damaged (expected $expected, got $actual). Try again."
}

# Shows what the script would do, for XILO_DRY_RUN=1.
dry_run() {
	say "Would download SHA256SUMS.txt and the file that ends with $suffix from $base"
	say "Would install Xilo to $1"
	say "Would add the xilo command to $2"
	if [ -z "${XILO_NO_OPEN:-}" ]; then
		say "Would open Xilo"
	fi
}

# Says how to add a folder to PATH, if it is not there yet.
check_path() {
	case ":$PATH:" in
	*":$1:"*) ;;
	*) say "Add $1 to your PATH to use the xilo command in a terminal." ;;
	esac
}

macos_app() {
	# Update a copy that you put in /Applications yourself. Otherwise install in your own Applications folder.
	if [ -d /Applications/Xilo.app ] && [ -w /Applications ]; then
		say /Applications/Xilo.app
	else
		say "$HOME/Applications/Xilo.app"
	fi
}

# /usr/local/bin if it is yours to write, as with Homebrew on Intel Macs.
macos_command_dir() {
	if [ -d /usr/local/bin ] && [ -w /usr/local/bin ]; then
		say /usr/local/bin
	else
		say "$HOME/.local/bin"
	fi
}

macos_stop() {
	if pgrep -x Xilo >/dev/null 2>&1; then
		say "Quitting the running Xilo..."
		osascript -e "quit app id \"$bundle_id\"" >/dev/null 2>&1 || true
		tries=0
		while pgrep -x Xilo >/dev/null 2>&1 && [ "$tries" -lt 10 ]; do
			sleep 1
			tries=$((tries + 1))
		done
		pkill -x Xilo 2>/dev/null || true
	fi
}

install_macos() {
	app=$(macos_app)
	command="$(macos_command_dir)/xilo"
	if [ -n "${XILO_DRY_RUN:-}" ]; then
		dry_run "$app" "$command"
		return
	fi

	fetch
	mkdir -p "$tmp/disk"
	hdiutil attach -nobrowse -readonly -noautoopen -quiet -mountpoint "$tmp/disk" "$tmp/$asset" || fail "could not open $asset."
	mounted="$tmp/disk"
	[ -d "$tmp/disk/Xilo.app" ] || fail "$asset does not contain Xilo.app."
	ditto "$tmp/disk/Xilo.app" "$tmp/Xilo.app"
	hdiutil detach -quiet "$tmp/disk" && mounted=""

	macos_stop
	mkdir -p "$(dirname "$app")"
	rm -rf "$app"
	mv "$tmp/Xilo.app" "$app"
	# curl does not set the quarantine flag, but remove it in case something else did.
	xattr -dr com.apple.quarantine "$app" 2>/dev/null || true
	say "Installed $app"

	mkdir -p "$(dirname "$command")"
	ln -sf "$app/Contents/Helpers/xilo" "$command"
	say "Added the xilo command: $command"

	say ""
	if [ -z "${XILO_NO_OPEN:-}" ]; then
		open "$app"
		say "Xilo is open. macOS asks once if Xilo may use Bluetooth: click Allow."
	fi
	say "To open Xilo when you log in, turn on Open at login in its Preferences."
	check_path "$(dirname "$command")"
}

linux_stop() {
	if pgrep -x xilo-app >/dev/null 2>&1; then
		say "Stopping the running Xilo..."
		pkill -x xilo-app 2>/dev/null || true
		tries=0
		while pgrep -x xilo-app >/dev/null 2>&1 && [ "$tries" -lt 10 ]; do
			sleep 1
			tries=$((tries + 1))
		done
	fi
}

# Copies next to the target first, so that the rename replaces the old file in one step.
put() {
	mkdir -p "$(dirname "$2")"
	cp "$1" "$2.new"
	chmod "$3" "$2.new"
	mv -f "$2.new" "$2"
}

install_linux() {
	bin_dir="$HOME/.local/bin"
	binary="$bin_dir/xilo-app"
	data="${XDG_DATA_HOME:-$HOME/.local/share}"
	entry="$data/applications/dev.xilo.Xilo.desktop"
	icon="$data/icons/hicolor/512x512/apps/dev.xilo.Xilo.png"
	if [ -n "${XILO_DRY_RUN:-}" ]; then
		dry_run "$binary, with an app menu entry" "$bin_dir/xilo"
		return
	fi

	fetch
	mkdir -p "$tmp/unpacked"
	tar -xzf "$tmp/$asset" -C "$tmp/unpacked"
	folder="$tmp/unpacked/${asset%.tar.gz}"
	[ -f "$folder/Xilo" ] || fail "$asset does not contain Xilo."

	linux_stop
	put "$folder/Xilo" "$binary" 755
	put "$folder/bin/xilo" "$bin_dir/xilo" 755
	if [ -f "$folder/xilo.png" ]; then
		put "$folder/xilo.png" "$icon" 644
	fi
	mkdir -p "$(dirname "$entry")"
	cat >"$entry" <<ENTRY
[Desktop Entry]
Type=Application
Name=Xilo
GenericName=Power Station Control
Comment=Control Anker SOLIX devices over Bluetooth
Exec="$binary"
Icon=dev.xilo.Xilo
Terminal=false
Categories=Utility;
Keywords=Anker;SOLIX;Bluetooth;Battery;Power;
StartupWMClass=dev.xilo.Xilo
ENTRY
	update-desktop-database "$data/applications" >/dev/null 2>&1 || true
	say "Installed $binary and the xilo command"

	say ""
	if ! has bluetoothctl; then
		say "Xilo needs BlueZ for Bluetooth. Install the bluez package with your package manager."
	fi
	if [ -n "${XILO_NO_OPEN:-}" ]; then
		:
	elif [ -n "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]; then
		nohup "$binary" >/dev/null 2>&1 &
		say "Xilo is open."
	else
		say "There is no desktop session, so Xilo did not open. Choose Xilo in your app menu."
	fi
	say "To open Xilo when you log in, turn on Open at login in its Preferences."
	check_path "$bin_dir"
	case "${XDG_CURRENT_DESKTOP:-}" in
	*GNOME*) say "On GNOME, the tray icon needs the AppIndicator extension." ;;
	esac
}

# Removes a symbolic link only if it points into Xilo.app.
remove_link() {
	case "$(readlink "$1" 2>/dev/null || true)" in
	*/Xilo.app/*) rm -f "$1" ;;
	esac
}

uninstall() {
	if [ "$os" = macos ]; then
		macos_stop
		remove_link /usr/local/bin/xilo
		remove_link "$HOME/.local/bin/xilo"
		rm -rf "$HOME/Applications/Xilo.app" "$HOME/Library/LaunchAgents/$bundle_id.plist"
		if [ -d /Applications/Xilo.app ]; then
			rm -rf /Applications/Xilo.app 2>/dev/null || say "Remove /Applications/Xilo.app yourself: you cannot write there."
		fi
		kept="$HOME/Library/Application Support/dev.xilo.Xilo"
	else
		linux_stop
		data="${XDG_DATA_HOME:-$HOME/.local/share}"
		config="${XDG_CONFIG_HOME:-$HOME/.config}"
		rm -f "$HOME/.local/bin/xilo-app" "$HOME/.local/bin/xilo" \
			"$data/applications/dev.xilo.Xilo.desktop" \
			"$data/icons/hicolor/512x512/apps/dev.xilo.Xilo.png" \
			"$config/autostart/dev.xilo.Xilo.desktop"
		kept="$config/xilo and $data/xilo"
	fi
	say "Xilo is uninstalled."
	say "Your settings and history are still in $kept. Delete them if you do not need them."
}

cleanup() {
	if [ -n "${mounted:-}" ]; then
		hdiutil detach -quiet -force "$mounted" 2>/dev/null || true
	fi
	rm -rf "$tmp"
}

main() {
	case "${1:-}" in
	"") action=install ;;
	--uninstall) action=uninstall ;;
	-h | --help)
		usage
		exit 0
		;;
	*) fail "unknown option: $1. Run with --help to see the options." ;;
	esac

	detect
	if [ "$action" = uninstall ]; then
		uninstall
		return
	fi

	tmp=$(mktemp -d 2>/dev/null || mktemp -d -t xilo)
	mounted=""
	trap cleanup EXIT
	trap 'exit 1' INT TERM

	if [ "$os" = macos ]; then
		install_macos
	else
		install_linux
	fi
}

# Everything runs from here, so that a partial download does nothing.
main "$@"
