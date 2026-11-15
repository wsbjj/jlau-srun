#!/bin/bash
# 打包成 DMG（拖拽安装）
# 用法: bash deploy/make_dmg.sh
set -euo pipefail

DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$DIR"

APP_NAME="JLAU校园网"
VERSION="1.0"
APP="$DIR/dist/$APP_NAME.app"
DMG="$DIR/dist/${APP_NAME}-${VERSION}.dmg"
STAGE="$DIR/dist/.dmg-stage"

echo "==> 重新打包 .app"
bash "$DIR/deploy/make_app.sh" >/dev/null

echo "==> 准备 DMG 内容"
rm -rf "$STAGE"
mkdir -p "$STAGE"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"

# 卷图标（失败无碍）
if [ -f "$DIR/dist/AppIcon.icns" ] && command -v SetFile >/dev/null 2>&1; then
    cp "$DIR/dist/AppIcon.icns" "$STAGE/.VolumeIcon.icns"
    SetFile -a C "$STAGE" 2>/dev/null || true
fi

echo "==> 生成 DMG"
rm -f "$DMG"
hdiutil create \
    -volname "$APP_NAME" \
    -srcfolder "$STAGE" \
    -fs HFS+ \
    -format UDZO \
    -ov \
    "$DMG" >/dev/null

rm -rf "$STAGE"

echo
echo "==> 完成: $DMG"
echo "    大小: $(du -h "$DMG" | cut -f1)"
echo "    SHA256: $(shasum -a 256 "$DMG" | cut -d' ' -f1)"
echo
echo "首次打开（去掉隔离属性）:"
echo "    xattr -dr com.apple.quarantine '/Applications/$APP_NAME.app'"
