# WebCodex Server-only Docker Compose 部署

这套 Compose 只部署协调服务器，不在服务器上运行 Runner，也不包含
Caddy、Nginx、Traefik 等反向代理。

```text
Internet
   │ HTTPS / WebSocket
   ▼
现有反向代理
   │ http://127.0.0.1:8080
   ▼
webcodex-server 容器
   │
   └── SQLite 与运行状态持久卷

代码所在电脑
   └── webcodex-runner ── HTTPS / WebSocket ──▶ 公网 WebCodex 地址
```

## 边界

服务器上只运行：

- `webcodex-server`
- 容器内的 `webcodex` 管理 CLI
- SQLite 数据卷

服务器上不运行：

- `webcodex-runner`
- Rust、Go、Node.js、Python 等项目工具链
- Git 仓库
- Caddy 或其他反向代理

Runner 仍然是 WebCodex 执行代码读取、修改、Git 操作和检查的必要组件，
但它应运行在真正持有代码仓库的本地电脑、工作站或独立开发机上。

## 1. 启动服务器容器

在仓库根目录执行：

```bash
./deploy/docker/bootstrap.sh https://webcodex.example.com
```

脚本会生成 `.env`、随机 Bootstrap Token，并执行：

```bash
docker compose up -d --build
```

默认只监听：

```text
127.0.0.1:8080
```

不会直接公开 WebCodex 端口。

也可以手动初始化：

```bash
cp .env.compose.example .env
chmod 600 .env
# 修改 WEBCODEX_PUBLIC_URL 和 WEBCODEX_TOKEN
docker compose up -d --build
```

## 2. 配置现有反向代理

反向代理的上游地址：

```text
http://127.0.0.1:8080
```

需要满足：

- 对外使用 HTTPS。
- 保留 `Host` 和 `X-Forwarded-Proto`。
- 支持 WebSocket Upgrade。
- `/api/agents/ws` 的连接超时应适合长连接。

最小 Nginx 示例：

```nginx
server {
    listen 443 ssl http2;
    server_name webcodex.example.com;

    # 使用你现有的证书配置

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_read_timeout 3600s;
        proxy_send_timeout 3600s;
        proxy_buffering off;
    }
}
```

如果反向代理本身也运行在 Docker 中，不要使用代理容器自己的
`127.0.0.1`。应让两个容器加入同一个外部 Docker 网络，或者让代理访问
宿主机映射端口。

## 3. 检查服务器

```bash
docker compose ps
docker compose logs -f webcodex
curl -fsS https://webcodex.example.com/openapi.json >/dev/null
```

入口：

```text
https://webcodex.example.com/console
https://webcodex.example.com/openapi.json
https://webcodex.example.com/mcp
```

## 4. 创建配对码

```bash
docker compose exec webcodex sh -lc \
  'webcodex pairing create \
    --server-url "$WEBCODEX_PUBLIC_URL" \
    --username admin \
    --ttl-secs 600'
```

只把短期有效的 `wc_pair_...` 配对码交给代码所在机器。不要复制服务器的
`WEBCODEX_TOKEN`。

## 5. 在代码所在机器运行 Runner

在持有代码仓库的机器安装 WebCodex：

```bash
npm install -g @yyjeqhc/webcodex
```

使用配对码登录，并限制允许访问的代码根目录：

```bash
sudo webcodex login https://webcodex.example.com \
  --code '<wc_pair_...>' \
  --allowed-root /home/your-user/git
```

按照 `login` 输出的实际配置路径安装 Runner 服务：

```bash
sudo webcodex agent install \
  --config /path/reported/by/login/agent.toml \
  --overwrite

sudo systemctl daemon-reload
sudo systemctl enable --now webcodex-runner
```

因此，公网服务器只负责协调；源代码、Git 凭据和项目工具链都留在代码机器。

## 常用管理命令

```bash
# 状态
docker compose ps

# 日志
docker compose logs -f webcodex

# 重启
docker compose restart webcodex

# 更新源码后重建
docker compose up -d --build

# 停止，但保留数据
docker compose down

# 删除服务器数据，谨慎执行
docker compose down -v
```
