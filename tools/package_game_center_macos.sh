#!/usr/bin/env bash
# Separate development app; does not overwrite the regular Todora package.
set -euo pipefail
cd "$(dirname "$0")/.."
export TODORA_APP_NAME="Todora Multiplayer"
export TODORA_FEATURES=game-center
exec bash tools/package_macos.sh
