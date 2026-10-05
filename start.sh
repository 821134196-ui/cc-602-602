#!/usr/bin/env bash
# 一条命令启动前后端：构建 Svelte 前端，然后启动 Axum 后端（后端托管静态文件）
set -e
cd "$(dirname "$0")"

if [ ! -d frontend/node_modules ]; then
  echo "==> 安装前端依赖..."
  (cd frontend && npm install)
fi

echo "==> 构建前端..."
(cd frontend && npm run build)

echo "==> 启动服务器（http://localhost:3000）..."
export PATH="$HOME/.cargo/bin:$PATH"
cargo run --release
