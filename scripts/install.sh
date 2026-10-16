#!/bin/sh
# Countdown installer, printed by `docker run --rm @DEFAULT_IMAGE@ install`.
# Usage: docker run --rm @DEFAULT_IMAGE@ install | sh
# It installs, in your home only (no root):
#   ~/.local/bin/countdown-run      starts the container with access to your display, sound and wallpaper
#   ~/.local/bin/countdown-toggle   shows the timer (bound to Ctrl+Alt+Space on GNOME)
#   ~/.config/autostart/countdown.desktop   starts it in the background at every login
set -eu
image="${COUNTDOWN_IMAGE:-@DEFAULT_IMAGE@}"
bin="$HOME/.local/bin"
autostart="$HOME/.config/autostart"
hotkey='<Control><Alt>space'

say() { printf 'countdown: %s\n' "$*" >&2; }

if [ "$(id -u)" = 0 ]; then
	say "run this as your desktop user, not root (pipe into 'sh', not 'sudo sh')."
	exit 1
fi
command -v docker >/dev/null || { say "docker not found"; exit 1; }
if [ -z "${DISPLAY:-}" ]; then
	say "no DISPLAY: run this from a terminal inside your desktop session."
	exit 1
fi

# Replace an older install that used a systemd user service.
if [ -f "$HOME/.config/systemd/user/countdown.service" ]; then
	systemctl --user disable --now countdown.service >/dev/null 2>&1 || true
	rm -f "$HOME/.config/systemd/user/countdown.service"
	systemctl --user daemon-reload >/dev/null 2>&1 || true
fi

mkdir -p "$bin" "$autostart"
cat > "$bin/countdown-run" <<'COUNTDOWN_EOF'
@RUN_SCRIPT@COUNTDOWN_EOF
sed -i "s|@IMAGE@|$image|" "$bin/countdown-run"
cat > "$bin/countdown-toggle" <<'COUNTDOWN_EOF'
@TOGGLE_SCRIPT@COUNTDOWN_EOF
chmod 755 "$bin/countdown-run" "$bin/countdown-toggle"

cat > "$autostart/countdown.desktop" <<COUNTDOWN_EOF
[Desktop Entry]
Type=Application
Name=Countdown
Comment=Frameless countdown timer (runs hidden; Ctrl+Alt+Space to show)
Exec=$bin/countdown-run
NoDisplay=true
X-GNOME-Autostart-enabled=true
COUNTDOWN_EOF

# Hotkey: GNOME custom keybinding, keeping any existing ones.
schema=org.gnome.settings-daemon.plugins.media-keys
kb_path=/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/countdown/
# Existing custom keybindings except ours. Also drops entries mangled by the 0.1.x installer
# (it stripped every "a", "s" and "@", leaving e.g. /org/gnome/etting-demon/…/countdown/).
other_bindings() {
	gsettings get $schema custom-keybindings | grep -o "'[^']*'" | tr -d "'" |
		grep -vx "$kb_path" | grep -v '^/org/gnome/etting-demon/plugin/medi-key/cutom-keybinding/' || true
}
if command -v gsettings >/dev/null && gsettings list-schemas 2>/dev/null | grep -qx "$schema"; then
	list=$(other_bindings)
	list=$(printf '%s\n%s\n' "$list" "$kb_path" | sed "/^$/d;s/.*/'&'/" | paste -sd, -)
	gsettings set $schema custom-keybindings "[$list]"
	gsettings set "$schema.custom-keybinding:$kb_path" name 'Countdown'
	gsettings set "$schema.custom-keybinding:$kb_path" command "$bin/countdown-toggle"
	gsettings set "$schema.custom-keybinding:$kb_path" binding "$hotkey"
	key_msg="Press Ctrl+Alt+Space"
else
	key_msg="Bind a shortcut (e.g. Ctrl+Alt+Space) to '$bin/countdown-toggle' in your keyboard settings, then press it"
fi

say "starting ($image)…"
"$bin/countdown-run"
say "installed. $key_msg, type the time (2 5 0 0 = 25:00), ↑/↓ to pick a sound, Enter."
say "remove with: docker run --rm $image uninstall | sh"
