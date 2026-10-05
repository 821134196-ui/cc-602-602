#!/usr/bin/env bash
# 一条命令启动前后端：构建 Svelte 前端，然后由 Axum 后端同时托管 API 与静态页面
set -euo pipefail
cd "$(dirname "$0")"

if [ ! -d web/node_modules ]; then
  echo "==> 安装前端依赖"
  (cd web && npm install)
fi

echo "==> 构建前端"
(cd web && npm run build)

echo "==> 启动后端（http://localhost:${PORT:-3000}）"
export PATH="$HOME/.cargo/bin:$PATH"
exec cargo run --manifest-path server/Cargo.toml "$@"
