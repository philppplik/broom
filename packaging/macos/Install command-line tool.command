#!/bin/bash
# Makes `broom` available in every terminal (asks for your password once).
APP="/Applications/Broom.app"
[ -d "$APP" ] || APP="$(cd "$(dirname "$0")" && pwd)/Broom.app"
sudo mkdir -p /usr/local/bin && sudo ln -sf "$APP/Contents/Resources/broom" /usr/local/bin/broom && echo "Done - run: broom"
