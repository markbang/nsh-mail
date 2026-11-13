[English](linux.md) | 简体中文

# 在 Linux 上使用 nsh-mail

## 需要什么

GTK 4.14 及以上、WebKitGTK 6.0（显示 HTML 邮件；没有它邮件以纯文本显示）、GStreamer 基础插件、glibc 2.39 及以上（在 Ubuntu 24.04 上构建），以及一款中日韩字体，否则中文邮件会显示成方块：

| 发行版 | 软件包 |
|---|---|
| Debian 13、Ubuntu 24.04+ | `libgtk-4-1 libwebkitgtk-6.0-4 gstreamer1.0-plugins-base fonts-noto-cjk` |
| Fedora | `gtk4 webkitgtk6.0 gstreamer1-plugins-base google-noto-sans-cjk-fonts` |
| Arch | `gtk4 webkitgtk-6.0 gst-plugins-base noto-fonts-cjk` |

GNOME 和 KDE 桌面一般都已装好。Ubuntu 22.04（GTK 4.6）太旧，不支持。

## 安装

```sh
mkdir -p ~/.local/opt ~/.local/bin ~/.local/share/applications
tar -xzf nsh-mail-<版本>-linux-x86_64.tar.gz -C ~/.local/opt
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

`chmod 444` 不能省：应用每次启动都会把可写的 `top.bangwu.nshmail.desktop` 换成一个隐藏、没有图标的条目（NSH 运行时的行为），nsh-mail 会从启动器里消失，通知也没有图标。设成只读后你的条目会保留，应用只记一条警告。`xdg-mime` 那一行让 `nshmail://` 链接（Google、Microsoft 登录的最后一步）由本应用打开。升级时把新压缩包解压覆盖旧目录即可。

## 桌面集成

- **托盘**：图标是 StatusNotifierItem，KDE Plasma、Waybar 和大多数面板都能显示；GNOME 需要 AppIndicator 扩展（Ubuntu 默认开启）。关闭窗口后 nsh-mail 仍在运行，用于新邮件通知；没有托盘时再次启动 nsh-mail 即可找回窗口，Ctrl+Q 退出。
- **通知**：窗口隐藏或不在前台时，新邮件通知交给桌面的通知服务。
- **密码**（本地模式）存进 Secret Service（GNOME Keyring、KWallet）；没有时加密保存在应用目录，应用会提示一次。
- **深色模式和字号**跟随桌面：配色经 XDG 设置门户实时跟随，字号取自字体缩放设置（`Xft.dpi`）。
