# Doubao Voice Bridge fcitx5 Addon

Wayland has no equivalent of the Windows `SendInput` path the desktop client
uses to write recognized speech into the focused application. mutter exposes
neither `zwp_virtual_keyboard_manager_v1` nor `zwp_input_method_manager_v2`, and
the RemoteDesktop portal's `NotifyKeyboardKeysym` only resolves keysyms that
already exist in the active keymap, so it cannot type CJK.

The mechanism that *can* write arbitrary text into any focused field on Linux is
the input method. This addon makes fcitx5 — which already owns the input focus
of every application on the desktop — commit the text the Doubao voice client
recognized, exactly the way it commits what a user typed. That mirrors the macOS
side, where the Doubao IME produces the text in the first place.

## What it publishes

The addon adds one object to fcitx5's existing bus connection, so callers
address it as `org.fcitx.Fcitx5` without a second well-known name:

| | |
| --- | --- |
| Service | `org.fcitx.Fcitx5` |
| Path | `/voicebridge` |
| Interface | `local.doubao.VoiceBridge1` |
| Methods | `CommitString(s text) -> b delivered`, `FocusedProgram() -> s` |

`delivered` is `false` when no application holds the input focus, which leaves
the text with the caller instead of dropping it. An empty `text` commits
nothing, so `CommitString("")` is a side-effect-free focus probe.
`FocusedProgram` names the focused application, which turns "the text went
nowhere" into a diagnosable answer.

## Build and install

fcitx5's headers and CMake modules are needed. On Arch Linux they ship in the
`fcitx5` package itself; Debian and Ubuntu split them into `fcitx5-modules-dev`.

```bash
cmake -S clients/fcitx5-addon -B build/fcitx5-addon -DCMAKE_INSTALL_PREFIX=/usr
cmake --build build/fcitx5-addon
sudo cmake --install build/fcitx5-addon
```

That writes `/usr/lib/fcitx5/libdoubaovoicebridge.so` and
`/usr/share/fcitx5/addon/doubaovoicebridge.conf`. fcitx5 only scans
`FCITX_ADDON_DIRS` (default `/usr/lib/fcitx5`) for addon libraries and has no
per-user addon directory, so a prefix inside `$HOME` also needs
`FCITX_ADDON_DIRS` set for the fcitx5 process.

Restart fcitx5 so it loads the addon, then confirm it is published:

```bash
fcitx5-remote -r
busctl --user introspect org.fcitx.Fcitx5 /voicebridge
```

The addon links against the fcitx5 ABI, so rebuild it after an fcitx5 major
upgrade.

## Uninstall

```bash
sudo rm /usr/lib/fcitx5/libdoubaovoicebridge.so \
        /usr/share/fcitx5/addon/doubaovoicebridge.conf
fcitx5-remote -r
```
