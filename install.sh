#!/bin/sh
# One-time setup: build the image, start Countdown at every login, bind Ctrl+Alt+Space.
# Usage: ./install.sh            install / update
#        ./install.sh --uninstall
set -eu
dir=$(cd "$(dirname "$0")" && pwd)
unit_dir="$HOME/.config/systemd/user"
bin="$HOME/.local/bin/countdown-toggle"
schema=org.gnome.settings-daemon.plugins.media-keys
kb_path=/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/countdown/
kb_schema="$schema.custom-keybinding:$kb_path"

bindings_without_ours() {
	gsettings get $schema custom-keybindings | tr -d "[]@as'" | tr ',' '\n' | sed 's/^ *//;/^$/d' | grep -vx "$kb_path" || true
}

set_bindings() {
	list=$(printf "%s\n" "$@" | sed "/^$/d;s/.*/'&'/" | paste -sd, -)
	gsettings set $schema custom-keybindings "[$list]"
}

if [ "${1:-}" = "--uninstall" ]; then
	systemctl --user disable --now countdown.service 2>/dev/null || true
	rm -f "$unit_dir/countdown.service" "$bin"
	systemctl --user daemon-reload
	set_bindings $(bindings_without_ours)
	gsettings reset-recursively "$kb_schema" 2>/dev/null || true
	(cd "$dir" && docker compose down --rmi local) || true
	echo "Countdown removed (settings volume kept: docker volume rm countdown_countdown-data)."
	exit 0
fi

echo "Building image…"
(cd "$dir" && docker compose build)

mkdir -p "$unit_dir" "$(dirname "$bin")"
sed "s|@DIR@|$dir|" "$dir/systemd/countdown.service.in" > "$unit_dir/countdown.service"
install -m 755 "$dir/bin/countdown-toggle" "$bin"
systemctl --user daemon-reload
systemctl --user enable countdown.service
systemctl --user restart countdown.service

set_bindings $(bindings_without_ours) "$kb_path"
gsettings set "$kb_schema" name 'Countdown'
gsettings set "$kb_schema" command "$bin"
gsettings set "$kb_schema" binding '<Control><Alt>space'

echo "Done. Press Ctrl+Alt+Space, type the minutes/seconds (e.g. 2 5 0 0), ↑/↓ for the sound, Enter."
