# Sabun

[![Release](https://img.shields.io/github/v/release/ryuapp/sabun?labelColor=171717&color=22b140&label=Release)](https://github.com/ryuapp/sabun/releases/latest)
[![License](https://img.shields.io/github/license/ryuapp/sabun?labelColor=171717&color=39b54a&label=License)](https://github.com/ryuapp/sabun/blob/main/LICENSE)
[![Platforms](https://img.shields.io/badge/Platforms-Windows%20%7C%20macOS%20%7C%20Linux-22b140?labelColor=171717)](https://github.com/ryuapp/sabun/releases)

Sabun is a blazing-fast diff viewer built with [GPUI](https://gpui.rs).
It's basically [Hunk](https://www.hunk.dev), but as a GUI without the TUI and agent stuff.

## Install

[mise](https://mise.jdx.dev) (recommended):

```sh
mise use -g github:ryuapp/sabun
```

manual:

Download the ZIP for your platform from
[GitHub Releases](https://github.com/ryuapp/sabun/releases), extract `sabun` (`sabun.exe` on
Windows), and place it somewhere on your `PATH`.

## Usage

Open the current repository's working-tree changes:

```sh
sabun        # Display the help
sabun diff   # Display current working-tree changes
```

Use the commands below to keep the view updated, focus on staged changes, open another
repository, or inspect commits and stashes:

```sh
sabun diff --watch                    # Reload automatically when files change
sabun diff --staged                   # Display staged changes
sabun diff --repo /path/to/repository # Display another repository
sabun show HEAD                       # Display the changes introduced by a commit
sabun stash show                      # Display the latest stash
```

Run `sabun` without a command to see all available commands and options.

## License

MIT-0
