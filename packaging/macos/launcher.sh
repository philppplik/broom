#!/bin/bash
# Broom.app launcher: opens Terminal and runs the bundled broom binary.
BIN="$(cd "$(dirname "$0")/../Resources" && pwd)/broom"
osascript -e "tell application \"Terminal\" to do script \"clear; '$BIN'; exit\"" -e 'tell application "Terminal" to activate' >/dev/null
