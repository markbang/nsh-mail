English | [简体中文](linux.zh-CN.md)

# nsh-mail on Linux

## What it needs

GTK 4.14 or later, WebKitGTK 6.0 (HTML mail; without it mail shows as text), the GStreamer base plug-ins, glibc 2.39 or later (built on Ubuntu 24.04), and a CJK font so Chinese mail does not show boxes:

| Distribution | Packages |
|---|---|
| Debian 13, Ubuntu 24.04+ | `libgtk-4-1 libwebkitgtk-6.0-4 gstreamer1.0-plugins-base fonts-noto-cjk` |
| Fedora | `gtk4 webkitgtk6.0 gstreamer1-plugins-base google-noto-sans-cjk-fonts` |
| Arch | `gtk4 webkitgtk-6.0 gst-plugins-base noto-fonts-cjk` |

GNOME and KDE installs usually have all of them. Ubuntu 22.04 (GTK 4.6) is too old.

## Install

```sh
mkdir -p ~/.local/opt ~/.local/bin ~/.local/share/applications
tar -xzf nsh-mail-<version>-linux-x86_64.tar.gz -C ~/.local/opt
ln -sf ~/.local/opt/nsh-mail/nshmail ~/.local/bin/nshmail

cat > ~/.local/share/applications/top.bangwu.nshmail.desktop <<EOF
[Desktop Entry]
Type=Application
Name=nsh-mail
Comment=All your mail in one inbox
Exec=$HOME/.local/opt/nsh-mail/nshmail %u
Icon=$HOME/.local/opt/nsh-mail/top.bangwu.nshmail.png
StartupWMClass=top.bangwu.nshmail
Terminal=false
Categories=Network;Email;
MimeType=x-scheme-handler/nshmail;
EOF
chmod 444 ~/.local/share/applications/top.bangwu.nshmail.desktop
xdg-mime default top.bangwu.nshmail.desktop x-scheme-handler/nshmail
```

The `chmod 444` matters: at every start the app replaces a writable `top.bangwu.nshmail.desktop` with a hidden entry that has no icon (an NSH runtime behavior), which takes nsh-mail out of the launcher and leaves notifications without its icon. Read-only, yours stays; the app only logs a warning. The `xdg-mime` line makes `nshmail://` links (the end of a Google or Microsoft sign-in) open the app. To update, extract the new archive over the old one.

## On the desktop

- **Tray**: the icon is a StatusNotifierItem, which KDE Plasma, Waybar and most panels show. GNOME shows it with the AppIndicator extension (Ubuntu has it on). Closing the window keeps nsh-mail running for new-mail notifications; without a tray, start nsh-mail again to bring the window back, and quit with Ctrl+Q.
- **Notifications** go to the desktop's notification service while the window is hidden or not focused.
- **Passwords** (local mode) go to the Secret Service (GNOME Keyring, KWallet). Without one they are kept encrypted in the app's folder, and the app says so once.
- **Dark mode and text size** follow the desktop: the color scheme through the XDG settings portal, live; the text size from the font scaling setting (`Xft.dpi`).
