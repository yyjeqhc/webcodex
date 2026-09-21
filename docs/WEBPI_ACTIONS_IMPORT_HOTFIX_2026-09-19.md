# WebPi ChatGPT Actions OpenAPI 导入热修复 — 2026-09-19

## 问题

ChatGPT Actions 编辑器导入 `https://webpi.piforme.vip/openapi.json` 时显示：

`In components section, schemas subsection is not an object`

生产 Schema 实测为 OpenAPI 3.1.0，`components` 是对象，但仅包含 `securitySchemes`，没有 `schemas`。

## 修复

`src/openapi.rs` 现在在存在 `components` 时显式输出空对象：

```json
"components": {
  "schemas": {},
  "securitySchemes": { "...": "..." }
}
```

同时更新 `scripts/webpi/security_smoke.py`：本机和公网部署验收现在要求 `components.schemas` 必须是 object，避免未来出现“服务健康但 Actions 导入器拒绝”的假阳性。对应 Python 回归覆盖非 object 情形。

## 验证

- `cargo test --profile dogfood --locked -p webcodex --lib openapi`：21 通过。
- `python -m unittest scripts.webpi.tests.test_security_smoke -v`：6 通过。
- `cargo fmt --all -- --check`：通过。
- 因生产 `webpi-server.exe` 被运行实例锁定，直接覆盖构建按预期失败；随后使用独立 `target/webpi-openapi-hotfix` 成功构建候选。
- 候选 Server SHA-256：`a19fbd13614c4380571e1c716731cad96519fc1cead7343689d1c52a84c076b0`。
- 用户手动停止精确路径下三个 WebPi 进程后，候选被安装至生产 `target/dogfood/webpi-server.exe`；当前生产文件哈希与候选一致。
- 旧生产 Server 备份：`.webpi-state/deployment-backups/openapi-import-hotfix-a7e43e70/webpi-server.exe.before`。
- 计划任务 `WebPi-Standalone` 已重新运行，Cloudflared 保持 Running。
- 本机安全验收通过；公网安全验收通过，12 个无效认证案例仍全部 401，生产 Action PAT 正向 runtime read 为 200。
- 公网 OpenAPI 实测：HTTP 200、`openapi=3.1.0`、`title=WebPi GPT Actions`、`components.schemas` 是 object 且当前为空、`securitySchemes` 是 object、共 27 个 paths。

## 边界

本轮没有更改生产凭据、数据库、Runner 配置或 Cloudflare 凭据，没有放宽鉴权，也没有操作 WebCodex Desktop。是否被 ChatGPT Actions UI 最终接受仍需在编辑器中重新导入当前 URL 做客户端侧确认。
