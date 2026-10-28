#!/usr/bin/env bash
# Installs Penwise for this user: release server in ~/.local/bin, viewer in ~/.local/share/penwise-app/web,
# a systemd user service that runs the server from login on, and a "Penwise" launcher (own app window).
# Re-run after pulling changes; it rebuilds and restarts the service. Data stays in ~/.local/share/penwise.
set -euo pipefail
cd "$(dirname "$0")/.."

bin="$HOME/.local/bin"
share="${XDG_DATA_HOME:-$HOME/.local/share}"
app="$share/penwise-app"
units="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"

cargo build --release -p penwise-server
(cd web && npm run wasm && npm run build)

mkdir -p "$bin" "$app" "$units" "$share/applications" "$share/icons/hicolor/scalable/apps"
install -m 755 target/release/penwise-server "$bin/penwise-server.new"
mv "$bin/penwise-server.new" "$bin/penwise-server"
rm -rf "$app/web.new"
cp -r web/dist "$app/web.new"
rm -rf "$app/web"
mv "$app/web.new" "$app/web"
install -m 755 desktop/penwise "$bin/penwise"
install -m 644 desktop/penwise.service "$units/penwise.service"
install -m 644 desktop/penwise.desktop "$share/applications/penwise.desktop"
install -m 644 desktop/penwise.svg "$share/icons/hicolor/scalable/apps/penwise.svg"

systemctl --user daemon-reload
systemctl --user enable penwise.service
systemctl --user restart penwise.service
update-desktop-database "$share/applications" 2>/dev/null || true

echo "Installed. Start \"Penwise\" from the app launcher (or run: penwise)."
echo "Server log: journalctl --user -u penwise -f"
