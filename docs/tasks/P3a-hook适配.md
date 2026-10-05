# 任务：3a hook 适配（`cairn hook claude|codex`）并删除探针

2026-10-05，cairn/main（主控）交给 cairn/dev-hook（Codex，常规：gpt-6-astra / high）。
路由：常规 / 交叉审查要 / 影响面：碰要害（路由：档拿不准（重 0.55、常规 0.45）、要、碰要害；档按规则取常规）
类型：功能变更
依据：本轮做唯一的 hook 入口：解析两种工具的官方 JSON、分派到已实现的核心函数、按各自格式输出；然后删除阶段 1 的探针。不做 install / uninstall / status（3b）、README（3c），不接触真实 agent 和真实配置。
提示：围绕已确认的使用目标完成变更，优先沿用现有机制。
你是被委派的 agent：照本文件做，不要再开别的 agent。

## 先读
- `AGENTS.md`（规矩一节，尤其隐私：hook 不保存 `prompt`、`last_assistant_message`，不读 `transcript_path`）
- `docs/DESIGN.md` §4（标准事件表）、§7（`cairn hook` 一行、退出码）、§8.1、§8.3、§8.4、**§8.6**、**§10.1、§10.2**（两种工具的事件、字段、输出格式）、§12
- `docs/调研/第一阶段能力实测.md` §3–§6、§10（实测到的字段和行为），`docs/调研/agent-hooks资料.md` 第 1、2 节
- 已合并的核心模块（直接调用，不改已有行为）：`session.rs`（SessionStarted）、`turn.rs`（TurnStarted / TurnEnded / SessionEnded，出错放行并带回错误）、`cli.rs`、`store.rs`、`spool.rs`
- 可以读 `/Users/firegnu/Developer/personal_projs/cairn-worktrees/p1-lab/logs/` 下的探针日志了解真实 JSON 的形状（都是合成会话、已脱敏）；夹具里只写字段名和合成值，不要复制原始 ID。

## 在哪里干活
- worktree：`/Users/firegnu/Developer/personal_projs/cairn-worktrees/p3a-hook`，分支 `p3a-hook`（已从 main 建好）。
- 新建模块（例如 `hook.rs`），在 `lib.rs` 声明；在 `cli.rs` 加 `hook` 子命令。夹具放 `crates/cairn/tests/fixtures/hooks/`，测试放 `crates/cairn/tests/` 下以 `hook` 开头的文件。
- 删除探针：从根 `Cargo.toml` 的 workspace members 去掉 `tools/cairn-probe`，删除 `tools/cairn-probe/` 目录，更新 `Cargo.lock`。在 hook 夹具测试通过之后再删。
- 编译：所有 cargo 命令前加 `CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target`。

## 要做的
- **`cairn hook <claude|codex>`**：读 stdin 的 JSON，按 `hook_event_name` 分派：
  - `SessionStart` → SessionStarted（`source` 映射成 start_kind：startup / resume / clear / compact，其余记 other）；
  - `UserPromptSubmit` → TurnStarted（turn_key 取 Claude 的 `prompt_id`、Codex 的 `turn_id`）；
  - `Stop` → TurnEnded（同上取 turn_key，`stop_hook_active` 作 continued）；
  - `SessionEnd` → SessionEnded；
  - Codex 的 `Interrupt`：只作观测（§10.2），不判定、不续跑；你决定记不记事件，写进完成记录；
  - 其他事件：不输出。
- **输出格式**照 §10.1 / §10.2：SessionStart 有注入时输出 `{"hookSpecificOutput":{"hookEventName":"SessionStart","additionalContext":"…"}}`；Stop 续跑时输出 `{"decision":"block","reason":"…"}`；Stop 放行时 Claude 不输出、Codex 输出 `{}`；其他事件不输出。
- **来源**：`claude:<session_id>` / `codex:<session_id>`。cwd 取 JSON 里的 `cwd`。
- **隐私**：不读、不保存、不记录 `prompt`、`last_assistant_message`；不打开 `transcript_path`。解析时可以忽略未知字段，但错误信息和日志里不能出现这些字段的内容。
- **`CAIRN_DISABLE=1`**：入口一开始就放行（按事件给出"放行"的格式），不读数据库、不收取。
- **失败策略**（§8.6）：任何错误（JSON 坏、数据库忙、git 超时、核心函数返回错误）都退出 0，按事件给出"放行"的格式；错误写进 `${XDG_STATE_HOME:-$HOME/.local/state}/cairn/errors.log`：只追加一行（时间、agent、事件名、错误类别和简短说明，不含正文和上面的隐私字段），文件 0600，只保留最近若干行（例如 200 行），写日志本身失败也不能影响退出码。不使用退出码 2。
- **时限**：SessionEnd 必须很快返回（Claude 默认 1.5 秒，Codex 设 3 秒），只写事件，不收取、不渲染。
- 生产路径的暂存区可信根用真实的 `DARWIN_USER_TEMP_DIR`；测试里沿用 2c 的做法把可信根指到临时目录，不要在真实临时目录下留下文件。

## 怎么算做完
以下验收条件经用户 2026-10-05 同意，取自实施计划 3a 的"建议验证"：
1. 用阶段 1 摘录的合成 JSON 做夹具：两种工具的 SessionStart、UserPromptSubmit、Stop（含 `stop_hook_active` 为真）、SessionEnd，以及 Codex 的 Interrupt，各有夹具和测试。
2. 未采用的项目：所有事件都不输出（Codex 的 Stop 输出 `{}`），不写数据库。
3. `CAIRN_DISABLE=1` 生效：不读数据库、不收取，按放行格式输出。
4. 两种工具的 Stop 输出格式正确：续跑与放行各一种，Claude 和 Codex 分别验证。

另外按任务要求：隐私字段不进数据库、错误日志和输出（用带唯一合成标记的 `prompt` / `last_assistant_message` 夹具检查）；出错时退出 0 且写错误日志；探针删除后 workspace 正常编译。

验证只做这些：上面各条一个测试（以端到端调用 `cairn hook` 子进程或 `cli::run_at` 为主），再补你判断直接相关的边角用例（完成记录里列出）；`cargo test --all-targets` 和 `cargo clippy --all-targets -- -D warnings` 各跑一次并通过。测试只用临时 `XDG_STATE_HOME` / `HOME` / 临时 Git 仓库。觉得不够，在回复里说，不要自己加。

## 不要做
- 不要运行真实的 `claude` 或 `codex`，不要开 agent；不要写 `~/.claude/settings.json`、`~/.codex/hooks.json`、`~/.codex/config.toml`，不要做 install。
- 不要读写真实的 cairn 数据库、真实 `~/.local/state`、真实临时目录下的 `cairn-spool`；不要读真实会话记录、工具记忆、Corral 事件文件。不用 Python。
- 不要改 session / turn / render / commands / store / schema / scope / facts / spool / ingest / save 的已有行为，不改表结构；需要小接口时停下来报告。DESIGN 说不通的地方停下来报告，等主控决定。
- 不要按项目名或路径批量杀进程。
- 不合并到 main，不推送。只在 `p3a-hook` 分支上提交。

## 做完
在本文件末尾追加「## 完成记录」（在你的分支里提交）：做了什么、验证了什么、拿主意的地方、没做的事，各几句话。回复里只写这几样，加上有没有要主控决定的事。命令都在前台跑完，全部做完后，回复最后一行写 DONE。
