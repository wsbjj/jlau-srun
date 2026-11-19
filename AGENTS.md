# AGENTS.md

吉林农业大学校园网自动登录。Rust 托盘在 `menubar/`。

## 触发 GitHub Actions 之前

推送或提交会触发 `.github/workflows/build.yml` 的改动之前，先在本机做 macOS 预检。包括推到 `main` / `master` 的代码、`v*` tag，以及会跑该 workflow 的 pull request。

只跑下面这两条，并且连续跑两遍。工作目录是 `menubar/`。两遍的退出码都要是 0，然后才能提交或推送。第一遍失败就先改代码，改完从第一遍重新开始。

```bash
cd menubar
cargo clippy --all-targets --all-features -- -D warnings
cargo test --release
cargo clippy --all-targets --all-features -- -D warnings
cargo test --release
```

这四条命令就是预检的全部。不跑 Linux、Windows、Docker、`act`，不安装 `libgtk-3-dev` 等 Linux 依赖，也不跑 `deploy/make_dmg.sh`。

只改 `**.md`、`docs/**`、`LICENSE`、`.gitignore` 时，workflow 的 `paths-ignore` 不会启动，不必做这个预检。
