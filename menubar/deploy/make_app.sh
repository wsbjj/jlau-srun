#!/bin/bash
# 打包成 .app（LSUIElement，无 Dock 图标）
# 用法: bash deploy/make_app.sh
set -euo pipefail

DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$DIR"

echo "==> 编译 release"
cargo build --release

APP="$DIR/dist/JLAU校园网.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

cp "$DIR/target/release/jlau-srun-menubar" "$APP/Contents/MacOS/jlau-srun-menubar"
chmod +x "$APP/Contents/MacOS/jlau-srun-menubar"

# 应用图标（「校」字，Swift 渲染）
if [ ! -f "$DIR/dist/AppIcon.icns" ]; then
    echo "==> 生成图标"
    bash "$DIR/tools/make_icns.sh" "$DIR/dist" >/dev/null
fi
cp "$DIR/dist/AppIcon.icns" "$APP/Contents/Resources/AppIcon.icns"

cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>JLAU校园网</string>
    <key>CFBundleDisplayName</key><string>JLAU 校园网</string>
    <key>CFBundleIdentifier</key><string>com.jlau.srun.menubar</string>
    <key>CFBundleExecutable</key><string>jlau-srun-menubar</string>
    <key>CFBundleIconFile</key><string>AppIcon</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleVersion</key><string>1.0</string>
    <key>CFBundleShortVersionString</key><string>1.0</string>
    <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>LSUIElement</key><true/>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

plutil -lint "$APP/Contents/Info.plist"
codesign --force --deep --sign - "$APP" >/dev/null 2>&1 || echo "（ad-hoc 签名失败，不影响本机运行）"

echo "==> 已生成: $APP"
echo "    启动: open \"$APP\""
echo "    自启: bash deploy/install_launchagent.sh"
