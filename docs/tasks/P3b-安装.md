# 任务：3b install / uninstall / status

2026-10-05，cairn/main（主控）交给 cairn/dev-install（Codex，常规：gpt-6-astra / high）。
路由：常规 / 交叉审查要 / 影响面：碰要害（路由：档拿不准（重 0.57、常规 0.43）、要、碰要害；档按规则取常规）
类型：功能变更
依据：本轮做 `cairn install` / `uninstall` / `status`，全部只在隔离 HOME 下测试；**不在用户真实环境里执行 install**（那要等用户明确同意，阶段 4 再做）。不做 README（3c）。
提示：围绕已确认的使用目标完成变更，优先沿用现有机制。
你是被委派的 agent：照本文件做，不要再开别的 agent。

## 先读
- `AGENTS.md`（规矩一节，尤其"不改用户真实配置"）
- `docs/DESIGN.md` §7（install / uninstall / status 一行）、**§10 全文**（两种工具的 hook 事件、输出、安装位置、信任、Corral 共存、§10.3 安装通则）、§12
- `docs/调研/第一阶段能力实测.md` §7（E：安装与信任，只换软链接目标不用重新信任、改命令文本要重新信任）、§14.4（Claude 的 save 规则要写在 settings 文件里，不能用 `--settings` 传）
- `docs/调研/agent-hooks资料.md` 第 1、2 节（配置文件位置与格式、超时）
- 阶段 1 实际用过的配置样例（合成、只读参考）：`/Users/firegnu/Developer/personal_projs/cairn-worktrees/p1-lab/configs/`（例如 `codex-project-limit6000.json`、`claude-settings-local-during-14-4.json`）
- 已合并的模块：`hook.rs`（`cairn hook claude|codex`）、`spool.rs`（待收取 / 残留数量）、`store.rs`（`database_path`、只读打开）、`scope.rs`、`cli.rs`

## 在哪里干活
- worktree：`/Users/firegnu/Developer/personal_projs/cairn-worktrees/p3b-install`，分支 `p3b-install`（已从 main 建好）。
- 新建模块（例如 `install.rs`、`status.rs`），在 `lib.rs` 声明，在 `cli.rs` 加子命令。测试放 `crates/cairn/tests/` 下以 `install` 或 `status` 开头的文件。可以加成熟的依赖（例如 `serde_json` 的 `preserve_order`）。
- 不要改其他已有模块的行为。
- 编译：所有 cargo 命令前加 `CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target`。

## 要做的
- **稳定命令路径**（§10.3）：`${XDG_DATA_HOME:-$HOME/.local/share}/cairn/bin/cairn` 是一个软链接，指向当前运行的 cairn 二进制（规范路径）。install 时创建或更新它；hook 和权限规则里一律写这个稳定路径，不写实际二进制路径。这样以后只换软链接目标，hook 文本不变，Codex 不用重新信任。
- **`cairn install --agent <claude|codex> [--dry-run | --yes]`**：
  - 先算出要改什么，打印出来（`--dry-run` 只打印、不改任何东西，包括不建软链接）；
  - 没有 `--dry-run` 时要确认：交互确认，或者带 `--yes`；stdin 不是终端且没带 `--yes` 就拒绝；
  - 改之前备份原文件（文件不存在就不用备份），备份文件名带时间、不覆盖已有备份；
  - 只合并 cairn 自己的条目，别的 hook 和设置原样保留（顺序、内容都不动）；cairn 的条目用命令文本识别；
  - 写文件要原子（写临时文件再改名），保留原文件权限；
  - 重复执行结果不变（幂等），不产生重复条目。
  - **Claude**：用户级 `~/.claude/settings.json`（尊重 Claude Code 支持的配置目录环境变量，如有；写进完成记录）。合并 SessionStart、UserPromptSubmit、Stop、SessionEnd 四个 hook，命令 `<稳定路径> hook claude`；再加一条权限规则 `Bash(<稳定路径> save:*)`（§10.1：必须写在 settings 文件里）。SessionEnd 的超时按资料设一个能在时限内完成的值。
  - **Codex**：`~/.codex/hooks.json`（尊重 `CODEX_HOME`，如有）。合并 SessionStart（handler 上设 `additionalContextLimit: 6000`）、UserPromptSubmit、Stop、SessionEnd（超时 3 秒）四个 hook，命令 `<稳定路径> hook codex`。**不改** `config.toml`、不改 sandbox / approval 设置（§10.2）。install 结束时提醒用户到 Codex 的 `/hooks` 里审核并信任。
- **`cairn uninstall --agent <claude|codex> [--dry-run | --yes]`**：同样先展示、确认、备份；只删 cairn 自己的条目（hook 和那条权限规则），别的不动。软链接是否删除你定（建议两种 agent 都卸载后才删），写进完成记录。
- **`cairn status [--json]`**（§10.3）：报告各 agent 是否已安装（条目在不在、命令是否指向稳定路径）、稳定路径的软链接是否有效、当前目录所在项目是否已采用（数据库不存在时报告"尚无数据"，不建库）、暂存区待收取的 `.json` 和残留 `.tmp` 数量及目录（用 2c 的函数）；Codex 已安装时提醒去 `/hooks` 确认信任状态。只读，不改任何东西。

## 怎么算做完
以下验收条件经用户 2026-10-05 同意，取自实施计划 3b 的"建议验证"：
1. 全部在隔离 HOME 下测试（临时 `HOME`、`XDG_*`、`CODEX_HOME` 等），不读写真实配置。
2. 已有的用户 hooks 保持不变：预先放一份带其他 hook 和其他设置的合成配置，install 后它们逐字保留。
3. 重复安装是幂等的：装两次，结果和装一次一样，没有重复条目。
4. 卸载只删自己的条目：uninstall 后配置与安装前等价（其他条目原样），备份文件存在。
5. status 报告准确：未安装、已安装、软链接失效、项目已采用 / 未采用、有待收取文件这几种状态各一个测试。

另外按任务要求：`--dry-run` 不改任何文件；非终端且没 `--yes` 时拒绝；备份不覆盖已有备份。

验证只做这些：上面各条一个测试，再补你判断直接相关的边角用例（完成记录里列出，例如原配置是坏 JSON 时拒绝而不覆盖）；`cargo test --all-targets` 和 `cargo clippy --all-targets -- -D warnings` 各跑一次并通过。觉得不够，在回复里说，不要自己加。

## 不要做
- **不要在真实 HOME 下运行 `cairn install` / `uninstall`**，不要读写 `~/.claude/settings.json`、`~/.claude.json`、`~/.codex/hooks.json`、`~/.codex/config.toml`，不要在真实的 `~/.local/share` 下建软链接。测试一律隔离。
- 不要运行真实的 `claude` 或 `codex`，不要开 agent；不要读真实会话记录、工具记忆、Corral 事件文件、真实 cairn 数据库。不用 Python。
- 不要改 sandbox / approval 配置，不要加任何扩大权限的规则（只有那一条 save 规则）。
- 不要改其他模块的已有行为，不改表结构；DESIGN 说不通的地方停下来报告，等主控决定。
- 不要按项目名或路径批量杀进程。
- 不合并到 main，不推送。只在 `p3b-install` 分支上提交。

## 做完
在本文件末尾追加「## 完成记录」（在你的分支里提交）：做了什么、验证了什么、拿主意的地方、没做的事，各几句话。回复里只写这几样，加上有没有要主控决定的事。命令都在前台跑完，全部做完后，回复最后一行写 DONE。
