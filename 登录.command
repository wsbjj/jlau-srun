#!/bin/bash
# 吉林农业大学校园网 · 一键登录（双击运行）
# 首次使用：把学号写进 config.json，然后双击本文件即可。
set -u
cd "$(dirname "$0")" || exit 1

PY="$(command -v python3 || echo /usr/bin/python3)"

# 若 config.json 不存在，用示例配置
if [ ! -f config.json ]; then
    cp config.example.json config.json
    echo "已生成 config.json，请先填写学号/密码（或钥匙串名）后重新运行。"
    read -r -p "按回车退出..." _
    exit 0
fi

"$PY" srun_login.py status >/dev/null 2>&1 && {
    echo "当前已在线，无需登录。"
    "$PY" srun_login.py status
    read -r -p "按回车退出..." _
    exit 0
}

echo "正在登录……"
if "$PY" srun_login.py daemon --once; then
    echo "登录完成。"
else
    echo "登录失败，请检查 config.json 里的账号/密码，或用 python3 srun_login.py login -u 学号 -p 密码 手动试。"
fi
read -r -p "按回车退出..." _
