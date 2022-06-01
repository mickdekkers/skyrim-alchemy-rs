#!/usr/bin/env bash
set -e

# Note: in ModOrganizer I created a shortcut with the following settings.
# Title: skyrim-alchemy-rs
# Binary: C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe
# Start in: \\wsl$\Ubuntu\home\mick\projects\skyrim-alchemy-rs
# Arguments: -command "& 'target\x86_64-pc-windows-gnu\debug\skyrim-alchemy-rs.exe' -v export-game-data --game-path 'F:/Skyrim Elysium Remastered/Stock Game' data/game_data.json 2>&1 | %{ \"$_\" } > skyrim-alchemy-rs-export.log"

powershell.exe -command 'Start-Process -Wait -NoNewWindow -FilePath "F:/Skyrim Elysium Remastered/ModOrganizer.exe" -ArgumentList "moshortcut://:skyrim-alchemy-rs"'
