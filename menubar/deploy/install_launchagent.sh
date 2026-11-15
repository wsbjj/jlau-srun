#!/bin/bash
# 安装 / 卸载菜单栏工具的开机自启（LaunchAgent）
# 用法: bash deploy/install_launchagent.sh [uninstall]
set -euo pipefail

DIR="$(cd "$(dirname "$0")/.." && pwd)"
APP="$DIR/dist/JLAU校园网.app"
LABEL="com.jlau.srun.menubar"
PLIST="$HOME/Library/LaunchAgents/$LABEL.plist"

if [ "${1:-}" = "uninstall" ]; then
    launchctl bootout "gui/$(id -u)/$LABEL" 2>/dev/null || launchctl unload "$PLIST" 2>/dev/null || true
    rm -f "$PLIST"
    echo "已卸载开机自启 ($LABEL)"
    exit 0
fi

[ -x "$APP/Contents/MacOS/jlau-srun-menubar" ] || {
    echo "找不到 $APP，请先运行: bash deploy/make_app.sh"
    exit 1
}

mkdir -p "$HOME/Library/LaunchAgents" "$HOME/Library/Logs"
sed -e "s#__APP__#$APP#g" -e "s#__HOME__#$HOME#g" \
    "$DIR/deploy/com.jlau.srun.menubar.plist.tmpl" > "$PLIST"

launchctl bootout "gui/$(id -u)/$LABEL" 2>/dev/null || true
launchctl bootstrap "gui/$(id -u)" "$PLIST"
launchctl enable "gui/$(id -u)/$LABEL"

echo "已安装并启动: $LABEL"
echo "  查看状态: launchctl print gui/$(id -u)/$LABEL | head -20"
echo "  错误日志: tail -f $HOME/Library/Logs/jlau-srun-menubar.err"
