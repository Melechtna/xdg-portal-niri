# xdg-portal-niri

A lightweight backend for [Niri](https://github.com/niri-wm/niri). It drives niri's Mutter-compatible ScreenCast APIs and presents a simple, per-monitor/per-application picker. This should remove the need for the gnome portal and its dependancies, which I felt were needless bloat for an otherwise excellent compositor.

This also uses GTK for its UI, so things like Noctallia or DMS, can apply thier themes to it. Ontop of this, it is also entirely built, installed, and run in userspace.

## Notable behavior
- **Ignores `types` requests**: The picker always offers both monitor and window capture regardless of what an application asks for, since most can stream either monitors or windows anyway without issue.
- **Previews only on the monitor tab**: The window/application tab intentionally has no live previews, as this tends to cause issues.
- **Only one picker**: A new picker cancels any previous in-flight request, this prevents applications (thanks OBS) from spamming a billion picker windows on startup.
- **2 bundled Icons**: Tabs and rows use bundled Font Awesome symbolic icons `fa-display-symbolic.svg` and `fa-window-maximize-symbolic.svg`.

## Build

```sh
just build
# or
cargo build --release
```

## Install

The `justfile` installs artifacts to user locations, copies portal configs, sets up a user systemd unit, enables it, and restarts `xdg-desktop-portal` so it's available immediately:

```sh
just install
```

What gets installed:
- Binary: `~/.local/share/xdg-portal-niri/xdg-portal-niri`
- Portal config: `~/.local/share/xdg-desktop-portal/portals/niriportal.portal`
- Portals override: `~/.config/xdg-desktop-portal/niri-portals.conf`
- Systemd user unit: `~/.config/systemd/user/xdg-portal-niri.service`

## Uninstall

```sh
just uninstall
```

## Troubleshooting

- **Picker doesn't appear / backend not running:** Check `systemctl --user status xdg-portal-niri` and that the bus name is owned: `busctl --user list | grep org.freedesktop.impl.portal.desktop.niriportal`.
- **No effect after changes:** Restart the frontend portal: `systemctl --user restart xdg-desktop-portal`. Sometimes holds stale caches.
- **Doesn't start after reboot:** Ensure the user systemd unit is enabled `systemctl --user is-enabled xdg-portal-niri`. `xdg-desktop-portal` may have stale caches at boot, a restart usually fixes it.
