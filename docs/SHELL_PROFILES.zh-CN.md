# Shell Profiles（Prepared Environment Snapshots）

[English](SHELL_PROFILES.md) | [简体中文](SHELL_PROFILES.zh-CN.md)

WebCodex **不会**保持一个持久 shell session。它会为每个 project/profile 准备一次 environment snapshot，然后每次命令都作为独立进程运行，并应用该 snapshot。

本文档说明 shell profiles 的工作方式、配置方法和安全边界。

> 适用于 `webcodex-agent`，即真正执行 shell commands 的 host agent。server 不会读取或保存 shell env values、`init_script` bodies 或 tokens。

## 1. 什么是 prepared shell env snapshot

执行项目命令时，agent 会：

1. 解析有效 profile：`project.shell_profile`，否则 `shell.default_profile`，否则 plain shell config。
2. 为 `project/cwd + profile` 准备一次 environment snapshot：启动 profile program，应用 `env_clear` 和 profile `env`，运行可选 `init_script`，并通过 `env -0` 捕获环境变量。
3. 按 `project/cwd + profile name` 缓存 snapshot。
4. 后续每次命令都以 fresh process 运行，并应用 cached snapshot。

因此没有 long-lived shell，不会每条命令都 `source`，默认也不会加载 `.bashrc` 或 `.profile`。

## 2. 为什么默认不 source `.bashrc` / `.profile`

WebCodex 有意不在准备 snapshot 时 source 交互式 shell 启动文件：

- `.bashrc` 可能很慢。
- 可能包含 prompt、`stty`、echo 等交互式命令，导致非交互式 capture 卡住或污染输出。
- 可能泄露或污染环境。
- 在不同 host/user 上不可复现。

应使用显式 shell profile。显式 profile 更容易审计、更快，也不依赖 agent 用户的交互式 shell 配置。

## 3. Rust / Cargo 示例

```toml
[shell]
default_profile = "rust"

[shell.profiles.rust]
program = "sh"
args = ["-c"]

[shell.profiles.rust.env]
PATH = "/root/.cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
CARGO_HOME = "/root/.cargo"
RUSTUP_HOME = "/root/.rustup"
```

该 profile 不需要 `init_script`，因为 env block 已经设置了 `PATH`、`CARGO_HOME` 和 `RUSTUP_HOME`。

## 4. Python venv 示例

```toml
[shell.profiles.py-venv]
program = "bash"
args = ["-lc"]
init_script = '''
source .venv/bin/activate
'''
```

`init_script` 是 project-relative：`.venv/bin/activate` 从项目根目录解析，因此每个项目可以激活自己的 venv。

## 5. Conda 示例

```toml
[shell.profiles.conda-ml]
program = "bash"
args = ["-lc"]
init_script = '''
source /opt/miniconda3/etc/profile.d/conda.sh
conda activate ml
'''
```

## 6. 将项目绑定到 profile

项目 TOML（`projects.d/<id>.toml`）可以指定 profile：

```toml
id = "paper-exp"
path = "/root/git/paper-exp"
shell_profile = "conda-ml"
```

## 7. 解析规则

有效 profile 按以下顺序选择：

1. `project.shell_profile`；否则
2. `shell.default_profile`；否则
3. fallback 到 plain shell config。

`listProjects` 会暴露 `shell_profile`、`resolved_shell_profile` 和 `shell_profile_status`（`configured` / `missing` / `not_configured` / `unknown`）。

## 8. 修改配置需要重启 agent

当前没有 reload API。修改 `agent.toml` 或 project TOML 后，需要重启 `webcodex-agent`。重启会丢弃已有 snapshots，并在下一次命令时 lazy re-prepare。

## 9. 安全提示

- **不要**在 `init_script` 中放 tokens。
- **不要**在 `init_script` 中 `echo`/`printf` secrets。
- `runtime_status`、`listAgents` 和 `listProjects` 只暴露 sanitized metadata：profile name、`has_init_script`、`env_keys_count`、`program`、`args_count`。
- 它们不会暴露 `init_script` bodies、env values、tokens、Authorization header、完整 `agent.toml` 或完整 env snapshot。
- Agent token 相关环境变量会从 child process environment 中剥离。
- `prepare` 使用 `env_clear` 和显式 inherited keys allowlist；profiles 必须声明所需 env。

## 10. Troubleshooting

运行本地 agent-config doctor 验证 shell profiles 和 project binding：

```bash
webcodex-cli doctor --agent-config /etc/webcodex/agent.toml
```

如需 strict diagnostics 或对特定项目做远程 roundtrip，请参考 [BUILD_INSTALL.zh-CN.md](BUILD_INSTALL.zh-CN.md) 与 [TROUBLESHOOTING.zh-CN.md](TROUBLESHOOTING.zh-CN.md)。
