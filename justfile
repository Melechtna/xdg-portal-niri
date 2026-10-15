# Where `just install` puts the binary. The unit file in `misc/` refers to it by
# absolute path, so if you change `bin_dir` you must change that too.
bin_dir := home_directory() / ".local/share/xdg-portal-niri"
portal_dir := home_directory() / ".local/share/xdg-desktop-portal/portals"
conf_dir := home_directory() / ".config/xdg-desktop-portal"
unit_dir := home_directory() / ".config/systemd/user"

default: build

# Build the optimised ("stable") binary.
build:
	cargo build --release

# The frontend restart below is not optional: xdg-desktop-portal caches the
# backend's properties for its whole lifetime, so an already-running frontend
# only notices a fresh install after a restart. Without it, it keeps the
# defaults it read at boot and rejects cursor modes

# Install the binary, the portal config, and a user service, then enable it.
install: build
	install -Dm755 target/release/xdg-portal-niri {{bin_dir}}/xdg-portal-niri
	install -Dm644 misc/niriportal.portal {{portal_dir}}/niriportal.portal
	install -Dm644 misc/niri-portals.conf {{conf_dir}}/niri-portals.conf
	install -Dm644 misc/xdg-portal-niri.service {{unit_dir}}/xdg-portal-niri.service
	systemctl --user daemon-reload
	systemctl --user enable --now xdg-portal-niri
	systemctl --user restart xdg-desktop-portal

uninstall:
	systemctl --user disable --now xdg-portal-niri 2>/dev/null || true
	rm -f {{unit_dir}}/xdg-portal-niri.service
	systemctl --user daemon-reload
	rm -f {{bin_dir}}/xdg-portal-niri
	rmdir {{bin_dir}} 2>/dev/null || true
	rm -f {{portal_dir}}/niriportal.portal
	rm -f {{conf_dir}}/niri-portals.conf
	systemctl --user restart xdg-desktop-portal 2>/dev/null || true

# Simply build the binary
run: build
	./target/release/xdg-portal-niri

check:
	cargo check
	cargo clippy --all-targets

fmt:
	cargo fmt

test:
	cargo test
