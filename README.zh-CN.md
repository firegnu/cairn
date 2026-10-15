# cairn

[English](README.md) | 简体中文

cairn 是给终端里的 coding agent 使用的工作接续记忆工具。它把上次会话停在哪里、现场有哪些可观测变化、建议的下一步保存下来，让新的 Claude Code 或 Codex 会话开局看到已有的摘要。会话安静满 3 分钟、会话结束，或同一条工作线上有新会话开局时，在后台写摘要。开局不等待；开局时已经停下的会话，其摘要若没赶上，会在你下一次发话时补进去。

cairn 读取 hook 给出的会话记录路径，调用已登录的 `codex exec` 写摘要，Claude Code 的会话也一样。使用摘要功能需要安装并登录 Codex；会话内容的摘录会交给 Codex，原文不存进 cairn 数据库。会话里的 agent 不再每回合保存，也不再为了保存而续跑。

cairn 独立运行，不依赖 Saddle、Corral 或 Drover。首版支持 Claude Code 和 Codex，只有明确采用的项目才会启用。

## 安装

需要 Rust stable。从源码安装：

```sh
cargo install --path crates/cairn
```

接入 agent 前，先查看将要做的改动：

```sh
cairn install --agent claude --dry-run
cairn install --agent codex --dry-run
```

确认后安装：

```sh
cairn install --agent claude --yes
cairn install --agent codex --yes
```

安装会创建稳定的命令路径 `${XDG_DATA_HOME:-$HOME/.local/share}/cairn/bin/cairn`，并更新配置。Claude 默认更新 `~/.claude/settings.json`（也可由 `CLAUDE_CONFIG_DIR` 指定目录），Codex 默认更新 `~/.codex/hooks.json`（也可由 `CODEX_HOME` 指定目录）。已有配置会先备份。Codex 安装完成后，在 Codex 中打开 `/hooks`，审核并信任 cairn 的 hook；仅项目目录信任或 `--yolo` 不会代替这一步。

## 采用项目

在要启用 cairn 的 Git 项目中运行：

```sh
cairn adopt
```

取消采用：

```sh
cairn unadopt
```

只有运行 `adopt` 的项目会接收和注入记录；没有采用的项目不受影响。

## 日常命令

- `cairn show`：显示当前工作线会注入给 agent 的接续内容。
- `cairn summarize`：在前台为当前工作线上该写摘要的会话写摘要，也可用于失败后重试。`show`、`list`、`status` 都不会触发写摘要。
- `cairn config`：查看摘要设置；用 `cairn config sidecar.model <模型>`、`cairn config sidecar.effort <强度>` 修改，默认分别为 `gpt-5.6-luna`、`medium`。
- `cairn list [--json]`：列出当前项目可见的记录（`--line` 只列当前工作线）。
- `cairn show <ID>`：显示指定记录的完整内容。
- `cairn correct <ID>`：为指定记录追加一条更正，正文从标准输入传入。
- `cairn retract <ID>`：追加声明，使指定记录默认不再显示。
- `cairn restore <ID>`：撤销指定记录的取代或撤回状态。
- `cairn delete <ID>`：删除指定记录的正文，只保留墓碑；需要时加 `--yes`。
- `cairn export`：把当前工作线的记录导出到标准输出，也可以指定一个新文件路径。
- `cairn status`：查看 agent 安装、项目采用状态和暂存区状态。

`cairn save` 留作自愿使用，或在你明确要求 agent 留一条记录时使用；正文通过标准输入传入，例如：

```sh
printf '%s\n' '## 停点' '完成了一个阶段。' | cairn save
```

## 数据与禁用

主数据库位于 `${XDG_STATE_HOME:-$HOME/.local/state}/cairn/cairn.db`。agent 受限保存入口先把内容写入用户私有临时目录的 `cairn-spool/`，再由 hook 收取；`cairn status` 会报告暂存区路径和未收取文件。

启动会话前设置 `CAIRN_DISABLE=1` 可将它排除在 cairn 之外：该会话的 hook 不注入内容，也不记录供自动摘要使用的事件和会话记录路径。

## 卸载与删除数据

卸载某个 agent 的接入配置：

```sh
cairn uninstall --agent claude --yes
cairn uninstall --agent codex --yes
```

卸载不会自动删除数据库。确认不再需要数据后，删除状态目录：

```sh
rm -rf "${XDG_STATE_HOME:-$HOME/.local/state}/cairn"
```

仍有未收取暂存文件时，先运行 `cairn status`，再按它报告的路径手动删除相应的 `cairn-spool` 内容。

记录是带来源的历史，不是当前指令或授权；执行前应核对现场，与用户本轮要求冲突时以用户为准。

## 从源码隔离走查

下面的命令在全新的临时环境中安装并走通基本流程，不会写入真实的用户配置。注意：暂存区位于系统给当前用户的私有临时目录（`getconf DARWIN_USER_TEMP_DIR`），不受 `TMPDIR` 影响；走查结束后那里会留下一个以数据库路径哈希命名的空目录 `cairn-spool/<哈希>/`，可以按 `cairn status` 报告的路径手动删除。

```sh
test_root="$(mktemp -d /tmp/cairn-readme.XXXXXX)"
export HOME="$test_root/home"
export XDG_STATE_HOME="$test_root/state"
export XDG_DATA_HOME="$test_root/data"
export CODEX_HOME="$test_root/codex"
export CLAUDE_CONFIG_DIR="$test_root/claude"
export TMPDIR="$test_root/tmp"
mkdir -p "$HOME" "$XDG_STATE_HOME" "$XDG_DATA_HOME" "$CODEX_HOME" "$CLAUDE_CONFIG_DIR" "$TMPDIR"

cargo install --path crates/cairn --root "$test_root/cargo"
export PATH="$test_root/cargo/bin:$PATH"

cairn install --agent claude --dry-run
cairn install --agent codex --dry-run
cairn install --agent claude --yes
cairn install --agent codex --yes

mkdir "$test_root/project"
cd "$test_root/project"
git init
cairn adopt
printf '%s\n' '## 停点' '完成隔离走查示例。' | cairn save
cairn show
cairn list
cairn status

cairn uninstall --agent claude --yes
cairn uninstall --agent codex --yes
```
