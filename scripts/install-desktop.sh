#!/usr/bin/env bash
# Installs Penwise for this user: release server in ~/.local/bin, viewer in ~/.local/share/penwise-app/web,
# a systemd user service that runs the server from login on, and a "Penwise" launcher (own app window).
# Re-run after pulling changes; it rebuilds and restarts the service. Data stays in ~/.local/share/penwise.
#
# Paths are fixed under ~/.local/share (the unit hard-codes the same ones); XDG_DATA_HOME is not honoured.
set -euo pipefail
cd "$(dirname "$0")/.."

bin="$HOME/.local/bin"
share="$HOME/.local/share"
app="$share/penwise-app"
units="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"

cargo build --release -p penwise-server
(cd web && npm run wasm && npm run build)

mkdir -p "$bin" "$app" "$units" "$share/applications" "$share/icons/hicolor/scalable/apps"
install -m 755 target/release/penwise-server "$bin/penwise-server.new"
mv "$bin/penwise-server.new" "$bin/penwise-server"
install -m 755 desktop/penwise "$bin/penwise"
install -m 644 desktop/penwise.service "$units/penwise.service"
# The menu entry names the launcher by absolute path: desktop sessions do not always have ~/.local/bin on PATH.
exec_path="$bin/penwise"
[[ "$exec_path" == *" "* ]] && exec_path="\"$exec_path\""
sed "s|^Exec=.*|Exec=$exec_path|" desktop/penwise.desktop >"$share/applications/penwise.desktop"
chmod 644 "$share/applications/penwise.desktop"
install -m 644 desktop/penwise.svg "$share/icons/hicolor/scalable/apps/penwise.svg"

systemctl --user daemon-reload
systemctl --user enable penwise.service
systemctl --user reset-failed penwise.service 2>/dev/null || true
systemctl --user restart penwise.service

# The viewer is served through `web`, a symlink to a versioned directory: the new server is already running,
# and the switch to the new viewer is one atomic rename, so there is no 404 window and no new viewer on an
# old server.
version="web-$(date +%Y%m%d%H%M%S)"
cp -r web/dist "$app/$version"
if [[ -d "$app/web" && ! -L "$app/web" ]]; then
    # First run after the layout change: the old plain directory becomes the first version.
    mv "$app/web" "$app/web-legacy"
    ln -s web-legacy "$app/web"
fi
previous="$(readlink "$app/web" 2>/dev/null || true)"
ln -s "$version" "$app/web.new"
mv -T "$app/web.new" "$app/web"
for old in "$app"/web-*; do
    name="$(basename "$old")"
    [[ "$name" == "$version" || "$name" == "$previous" ]] || rm -rf "$old"
done

update-desktop-database "$share/applications" 2>/dev/null || true

echo "Installed. Start \"Penwise\" from the app launcher (or run: penwise)."
echo "Server log: journalctl --user -u penwise -f"
