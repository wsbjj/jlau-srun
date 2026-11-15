#!/bin/bash
# macOS 开机自启安装脚本
# 用法: bash deploy/install_macos.sh
# 卸载: bash deploy/install_macos.sh uninstall
set -euo pipefail

DIR="$(cd "$(dirname "$0")/.." && pwd)"
LABEL="com.jlau.srun"
PLIST="$HOME/Library/LaunchAgents/$LABEL.plist"
PY="$(command -v python3 || echo /usr/bin/python3)"

if [ "${1:-}" = "uninstall" ]; then
    launchctl bootout "gui/$(id -u)/$LABEL" 2>/dev/null || launchctl unload "$PLIST" 2>/dev/null || true
    rm -f "$PLIST"
    echo "已卸载 $LABEL"
    exit 0
fi

[ -f "$DIR/config.json" ] || { echo "缺少 $DIR/config.json，请先 cp config.example.json config.json 并填写"; exit 1; }

mkdir -p "$HOME/Library/LaunchAgents" "$HOME/Library/Logs"
sed -e "s#__PY__#$PY#g" -e "s#__DIR__#$DIR#g" -e "s#/Users/USERNAME#$HOME#g" \
    "$DIR/deploy/com.jlau.srun.plist.tmpl" > "$PLIST"

launchctl bootout "gui/$(id -u)/$LABEL" 2>/dev/null || true
launchctl bootstrap "gui/$(id -u)" "$PLIST"
launchctl enable "gui/$(id -u)/$LABEL"
echo "已安装并启动: $LABEL"
echo "  状态: launchctl print gui/$(id -u)/$LABEL | head -30"
echo "  日志: tail -f $HOME/Library/Logs/jlau-srun.log"
