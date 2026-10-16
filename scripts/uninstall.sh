#!/bin/sh
# Countdown uninstaller, printed by `docker run --rm @DEFAULT_IMAGE@ uninstall`.
# Usage: docker run --rm @DEFAULT_IMAGE@ uninstall | sh
set -u
image="${COUNTDOWN_IMAGE:-@DEFAULT_IMAGE@}"

docker rm -f countdown >/dev/null 2>&1
rm -f "$HOME/.local/bin/countdown-run" "$HOME/.local/bin/countdown-toggle" "$HOME/.config/autostart/countdown.desktop"
if [ -f "$HOME/.config/systemd/user/countdown.service" ]; then
	systemctl --user disable --now countdown.service >/dev/null 2>&1
	rm -f "$HOME/.config/systemd/user/countdown.service"
	systemctl --user daemon-reload >/dev/null 2>&1
fi

schema=org.gnome.settings-daemon.plugins.media-keys
kb_path=/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/countdown/
# Existing custom keybindings except ours. Also drops entries mangled by the 0.1.x installer
# (it stripped every "a", "s" and "@", leaving e.g. /org/gnome/etting-demon/…/countdown/).
other_bindings() {
	gsettings get $schema custom-keybindings | grep -o "'[^']*'" | tr -d "'" |
		grep -vx "$kb_path" | grep -v '^/org/gnome/etting-demon/plugin/medi-key/cutom-keybinding/' || true
}
if command -v gsettings >/dev/null && gsettings list-schemas 2>/dev/null | grep -qx "$schema"; then
	list=$(other_bindings | sed "s/.*/'&'/" | paste -sd, -)
	gsettings set $schema custom-keybindings "[$list]"
	gsettings reset-recursively "$schema.custom-keybinding:$kb_path" 2>/dev/null
fi

printf 'countdown: removed. Saved settings: docker volume rm countdown-data; image: docker rmi %s\n' "$image" >&2
