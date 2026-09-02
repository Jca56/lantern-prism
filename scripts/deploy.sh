#!/usr/bin/env sh
# Build Prism in release and install it where Lantern's apps live:
#   ~/.lantern/bin/lantern-prism             the binary (`prism` renamed)
#   ~/.lantern/icons/lantern-prism.svg       the app icon
#   ~/.local/share/applications/…desktop     the launcher entry
# Run after every change that should reach the desktop: `scripts/deploy.sh`.
set -eu
cd "$(dirname "$0")/.."
cargo build --release -p prism-app
install -Dm755 target/release/prism "$HOME/.lantern/bin/lantern-prism"
install -Dm644 deploy/lantern-prism.svg "$HOME/.lantern/icons/lantern-prism.svg"
install -Dm644 deploy/lantern-prism.desktop "$HOME/.local/share/applications/lantern-prism.desktop"
echo "deployed: $HOME/.lantern/bin/lantern-prism ($(du -h target/release/prism | cut -f1))"
