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

## 完成记录

实现者：cairn/dev-install（Codex），2026-10-05，分支 `p3b-install`。

### 做了什么

- 新增 `install.rs`、`install_json.rs`、`status.rs`，接入 install / uninstall / status CLI。配置修改先完整预览，再经终端确认或 `--yes` 应用；`--dry-run` 不建目录、文件或软链接。配置写入使用同目录临时文件原子改名，保留原权限；已有配置在修改前保存带 UTC 时间和随机后缀、独占创建的备份。
- 安装四个指定事件及 Claude 的单条 save 规则；Codex SessionStart 的 handler 配置 `additionalContextLimit: 6000`，SessionEnd 超时 3 秒；Claude SessionEnd 超时 1 秒，落在资料所述 1.5 秒共享预算内。Codex 安装后和 status 中均提醒 `/hooks` 审核信任。
- 稳定软链接使用 XDG_DATA_HOME / HOME 推导的路径，目标是当前二进制的规范路径。重复安装配置逐字不变；仅更新软链接目标时不改 hook 定义。JSON 修改复用未变成员的原始文本，保留其他设置的顺序、空白、数值表示和转义字符，不用整份重新格式化代替增量合并。
- status 提供文字 / JSON：两种 agent 各事件的条目存在情况、命令是否指向稳定路径、Claude save 规则、软链接与目标有效性、项目采用状态、暂存区目录和 `.json` / `.tmp` 数量。新增 `Spool::inspect` 仅打开已有目录，并复用 2c 的计数及权限检查；不改变原有 `Spool::open` 行为。

### 验证了什么

- 新增 8 项进程级 CLI 集成测试（`crates/cairn/tests/install.rs`）。每个测试通过 `env_clear` 建立临时 HOME、XDG_DATA_HOME、XDG_STATE_HOME、XDG_CONFIG_HOME、CODEX_HOME、CLAUDE_CONFIG_DIR、TMPDIR，并隔离 Git 全局配置。暂存区沿用 §6.5 的系统私有根，命名空间由临时数据库路径隔离，计数测试只清理自己的命名空间。
- 验收覆盖：合成用户 hook / 设置的原始字节保留；重复安装与软链接换目标；卸载只删自身且原配置等价；安装和卸载备份并存、不覆盖；未安装 / 已安装 / 失效软链接；项目已采用 / 未采用 / 尚无数据库；待收取与残留文件准确计数且不收取、不改数据库主文件；dry-run 不写，非终端未确认拒绝。
- 直接相关边角：坏 JSON 拒绝且不覆盖；混合 handler 中修正旧 cairn 路径并保留其他 handler；空 settings 无自身条目时卸载不改文件；路径中的空格 / 单引号正确 shell 引用；稳定路径被普通文件占用时拒绝；配置文件是软链接时拒绝替换。权限规则路径含通配符等不支持字符时拒绝，避免意外扩大 save 权限。
- RED → GREEN：最初 install / uninstall / status 分别因缺少对应子命令失败；旧 cairn 路径状态检测因漏报失败；空对象卸载因删除无关空设置失败。各次均在最小实现或修复后通过，没有用语法错误作为 RED。
- 最终检查：`CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target cargo test --all-targets` 一次通过，共 99 项；`CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target cargo clippy --all-targets -- -D warnings` 一次通过。`cargo fmt --all`、`git diff --check` 通过。所有命令均等待前台进程结束。

### 拿主意的地方

- 尊重 `CLAUDE_CONFIG_DIR`（[Claude 官方配置位置说明](https://code.claude.com/docs/en/settings)），默认 `~/.claude/settings.json`；尊重 `CODEX_HOME`，默认 `~/.codex/hooks.json`；空环境变量按未设置处理，非绝对配置目录拒绝。
- 只识别直接执行的绝对路径 `.../cairn hook <agent>` 及 `Bash(.../cairn save:*)`，支持 shell 引号。修正和删除自身条目时不把仅包含 cairn 字样的其他命令当成自己的条目；不处理包装脚本或复合命令。
- 卸载始终保留稳定软链接：避免影响另一种 agent 或用户现有命令引用；不会删除二进制、状态库或暂存文件。
- **主控已在本轮明确决定**：允许 `status` 沿用 `Store::open_read_only` 的 SQLite WAL/SHM 辅助文件行为，业务数据不变。status 不建数据库、不收取暂存区、不创建暂存区目录、不读取 Codex 信任配置；没有使用会忽略并发变化的 immutable 模式绕过 SQLite 锁。

### 没做的事 / 待决定

- 未运行真实 agent，未在真实 HOME 安装 / 卸载，未读写真实工具配置、会话、工具记忆或 cairn 数据库；未改 config.toml、sandbox / approval、表结构、原模块既有行为、README、HANDOFF 或 DESIGN。
- 未合并 main、未推送、未清理主控 worktree / agent。无新增待主控决定事项；后续交叉审查、合并及真实环境安装由主控按原流程推进。

## 返工记录

2026-10-05，按主控转交的交叉审查 R1–R3 返工；主仓库的交叉审查文件只读，未修改。

- **R1**：认领前先检查引号和转义，仅接受无 shell 运算符或展开的字面量单词，再核对绝对路径、`cairn` 文件名及恰好两个参数 `hook <agent>`。未引用的控制符、通配符、变量/命令展开、注释或附加参数均不认领；合法引号中的空格、单引号仍支持。install、uninstall、status 共用这条认领规则，save 权限条目的路径也使用相同的字面量检查。回归覆盖审查中的 `/usr/bin/true;/opt/user/cairn hook claude`、相关运算符/展开/注释，并验证两种 agent 的配置原文保留和 status 不误认。
- **R2**：在合并入口递归检查所有对象的原始成员，按解码后的键检测重复；重复时报 `duplicate object key`，在配置、备份、目录或软链接写入前退出。回归覆盖审查原文、数组内嵌对象及 Unicode 转义后重名的键，确认 install/uninstall 均拒绝，原文件字节及权限不变，没有新增文件、备份、目录或链接。
- **R3**：保持现有清理行为：删除最后一条 cairn entry 时，清理随之为空的事件数组、hooks 对象、allow 数组和 permissions 对象，不区分空容器原先存在还是安装新增。因此安装/卸载的“等价”指 hook 和权限语义等价，不保证原有空容器的结构往返不变。已补审查中的 `{"hooks":{"Stop":[]},"permissions":{"allow":[]},"theme":"dark"}` 往返用例，断言最终仅剩 `{"theme":"dark"}`；没有 cairn 条目时直接卸载仍保留原有空容器。
- **验证**：R1 首次因 status 将复合命令认成自身 handler 而失败，R2 首次因 install 接受重复键而失败，修复后转绿；R3 为既有行为刻画，不制造 RED。`cargo test --test install review_r` 的 3 项回归通过，均使用已有隔离 HOME / XDG / 工具配置目录夹具，未执行合成 hook 命令。最终 `CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target cargo test --all-targets` 一次通过，共 102 项；`CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target cargo clippy --all-targets -- -D warnings` 一次通过。所有命令均等待前台进程结束。
- **范围**：仅修改命令认领、安装 JSON 检查、上述回归和本记录；无新依赖，不改变 R3 清理行为、DESIGN 或已确认的 SQLite 辅助文件边界。未读写真实配置/会话/数据，未运行真实 agent，未修改主仓库审查文件，未合并或推送；无新增待主控决定事项。
