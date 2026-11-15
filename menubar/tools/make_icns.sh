#!/bin/bash
# 生成 AppIcon.icns（由「校」字渲染）
# 用法: bash tools/make_icns.sh [输出目录]
set -euo pipefail

DIR="$(cd "$(dirname "$0")/.." && pwd)"
OUT_DIR="${1:-$DIR/dist}"
mkdir -p "$OUT_DIR"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

swift "$DIR/tools/make_icon.swift" "$TMP/icon.png"

ICONSET="$TMP/AppIcon.iconset"
mkdir -p "$ICONSET"
for s in 16 32 128 256 512; do
    sips -z "$s" "$s" "$TMP/icon.png" --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
    sips -z $((s * 2)) $((s * 2)) "$TMP/icon.png" --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done

iconutil -c icns "$ICONSET" -o "$OUT_DIR/AppIcon.icns"
echo "已生成 $OUT_DIR/AppIcon.icns"
