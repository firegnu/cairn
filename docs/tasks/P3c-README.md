# 任务：3c 安装方式与 README

2026-10-05，cairn/main（主控）交给 cairn/dev-readme（Codex，轻：gpt-5.6-luna / medium）。
路由：轻 / 交叉审查不要 / 影响面：看得见（路由：轻、要、碰要害；推翻后两项：本任务只写 README 并在隔离 HOME 里照做一遍，不改代码；README 写错一眼可见、随时可改；真实安装在阶段 4 由主控在用户同意后进行）
类型：样式／文案调整
依据：本轮只写仓库根的 README，并在全新的隔离 HOME 里照 README 从头走一遍；不改任何代码，不在真实环境里安装。
提示：沿用现有视觉和用语约定，聚焦指定的呈现结果。
你是被委派的 agent：照本文件做，不要再开别的 agent。

## 先读
- `AGENTS.md`（规矩一节，尤其"不改用户真实配置"）
- `docs/DESIGN.md` §1、§7、§10（尤其 §10.2 Codex 的 `/hooks` 信任、§10.3 稳定命令路径）、§12
- 各命令的实际用法以代码为准：`cargo run -p cairn -- --help` 及各子命令的 `--help`

## 在哪里干活
- worktree：`/Users/firegnu/Developer/personal_projs/cairn-worktrees/p3c-readme`，分支 `p3c-readme`（已从 main 建好）。
- 只新建 / 修改仓库根的 `README.md`，以及本任务文件末尾的完成记录。
- 编译：cargo 命令前加 `CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target`。

## 要做的
- **README.md**（中文，简洁），包括：
  - cairn 是什么（一两段，照 DESIGN §1 的意思）；
  - 从源码安装：`cargo install --path crates/cairn`；
  - 接入 agent：`cairn install --agent claude`、`cairn install --agent codex`（先 `--dry-run` 看改动；Codex 装完要到 `/hooks` 里信任）；说明会改哪些文件、会先备份；
  - 采用项目：`cairn adopt` / `unadopt`；没采用的项目不受影响；
  - 日常会用到的命令：`show`、`list`、`show <ID>`、`correct`、`retract`、`restore`、`delete`、`export`、`status`，各一句；
  - 数据放在哪（数据库、暂存区），以及 `CAIRN_DISABLE=1` 的用途；
  - 卸载：`cairn uninstall --agent …`，以及如何删除数据；
  - 一句说明：记录是带来源的历史，不是指令或授权（D7）。
- **照 README 走一遍**：在全新的临时目录里设好隔离环境（`HOME`、`XDG_STATE_HOME`、`XDG_DATA_HOME`、`CODEX_HOME`、Claude 配置目录变量、`TMPDIR`，以及 `cargo install` 用 `--root` 指向临时目录），然后严格按 README 的命令顺序执行：安装 → `install --agent claude --yes` 和 `--agent codex --yes` → 在临时 Git 仓库里 `adopt` → `printf ... | cairn save ...` → `show` → `list` → `status` → `uninstall` 两个 agent。README 里哪一步照做不通，就改 README（不改代码），直到能从头走通。

## 怎么算做完
以下验收条件经用户 2026-10-05 同意，取自实施计划 3c 的"建议验证"：
- 在全新的隔离 HOME 里按 README 从头走一遍，每一步都成功。

验证只做这些：上面这一遍走查（完成记录里写清每一步的命令和结果摘要）；`git diff --check`。不需要跑 cargo test。觉得不够，在回复里说，不要自己加。

## 不要做
- **不要在真实 HOME 下运行 `cairn install` / `uninstall` / `adopt`**，不要读写真实 `~/.claude*`、`~/.codex`、`~/.local/share/cairn`、`~/.local/state/cairn`，不要把 cairn 装进真实的 `~/.cargo/bin`。走查时每条命令都确认环境变量已指向临时目录。
- 不要改任何代码；README 和代码对不上时以代码为准写 README，确实是代码 bug 就停下来报告。
- 不要运行真实的 `claude` 或 `codex`，不要开 agent。不用 Python。
- 不要按项目名或路径批量杀进程。
- 不合并到 main，不推送。只在 `p3c-readme` 分支上提交。

## 做完
在本文件末尾追加「## 完成记录」（在你的分支里提交）：做了什么、走查的每一步与结果、拿主意的地方、没做的事，各几句话。回复里只写这几样，加上有没有要主控决定的事。命令都在前台跑完，全部做完后，回复最后一行写 DONE。

## 完成记录

### 做了什么

- 新建仓库根 `README.md`，用中文说明 cairn 的用途、源码安装、Claude Code/Codex 接入与备份、Codex `/hooks` 信任、项目采用、日常命令、数据位置、`CAIRN_DISABLE=1`、卸载和数据删除。
- README 末尾加入隔离走查命令，明确设置临时 `HOME`、XDG 目录、`CODEX_HOME`、`CLAUDE_CONFIG_DIR`、`TMPDIR`，并用 `cargo install --root` 安装。

### 走查步骤与结果

在全新的临时目录 `/tmp/cairn-readme.5VRWxY` 中，使用隔离配置目录并将共享编译目录设为 `/Users/firegnu/Developer/personal_projs/cairn-worktrees/.target`，按 README 顺序执行：

1. `cargo install --path crates/cairn --root "$test_root/cargo"`：成功安装 `cairn`。
2. `cairn install --agent claude --dry-run`：成功，显示 Claude `settings.json`、hook 和 save 权限改动。
3. `cairn install --agent codex --dry-run`：成功，显示 Codex `hooks.json` 和 hook 改动。
4. `cairn install --agent claude --yes`：成功，创建稳定软链接并写入隔离 Claude 配置。
5. `cairn install --agent codex --yes`：成功，写入隔离 Codex 配置，并提示到 `/hooks` 审核信任。
6. 在临时 Git 仓库执行 `cairn adopt`：成功，项目显示为已采用。
7. `printf '%s\\n' '## 停点' '完成隔离走查示例。' | cairn save`：成功，生成一条记录。
8. `cairn show`：成功显示注入抬头和刚保存的记录。
9. `cairn list`：成功列出该记录。
10. `cairn status`：成功报告两个 agent 已安装、稳定软链接有效、项目已采用、暂存区无待收取或残留文件。
11. `cairn uninstall --agent claude --yes`：成功，备份并清空隔离 Claude 配置。
12. `cairn uninstall --agent codex --yes`：成功，备份并清空隔离 Codex 配置。

### 拿主意的地方

- 按设计和实际帮助保留稳定命令路径、配置备份、Codex `/hooks` 信任提醒，以及 `cairn save` 通过标准输入接收正文的写法。
- 数据删除只在 README 中说明，不在走查中执行删除；走查严格覆盖任务要求的命令顺序。

### 没做的事

- 未修改任何 Rust 代码，未运行真实 Claude 或 Codex，未写入真实 HOME 或真实 agent 配置。
- 未运行 `cargo test`；任务明确只要求隔离走查和 `git diff --check`。

### 主控决定

- 无。

## 主控审查

2026-10-05，cairn/main。结论：通过，已合并。
- 只改了 README 和完成记录；README 内容与代码一致，覆盖安装、接入、采用、日常命令、数据、禁用、卸载与 D7 说明；`git diff --check` 通过。
- 核对真实环境：真实 `~/.local/share/cairn`、`~/.local/state/cairn`、`~/.cargo/bin/cairn` 不存在，真实 Claude / Codex 配置无 cairn 条目。
- 发现：暂存区取 `DARWIN_USER_TEMP_DIR`，不受 `TMPDIR` 影响，隔离走查在真实私有临时目录留下了空目录 `cairn-spool/7262111286db64b8/`（无文件）。主控在 README 补了说明（同分支提交）；该空目录删不删交用户决定。
