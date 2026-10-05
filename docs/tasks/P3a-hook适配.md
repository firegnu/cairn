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

## 完成记录

2026-10-05，cairn/dev-hook 完成。

**做了什么**：新增 `hook.rs`，`cairn hook claude|codex` 从 stdin 选择性解码事件元数据，直接调用既有 session / turn 接口；来源、cwd、回合 ID、continued、启动类型和两种输出格式按本任务映射。CLI 在解析用户命令所需的 cwd / 状态路径之前分流 hook；二进制 hook 输出不附带用户命令的换行，失败退出 0。入口先检查禁用开关，只解析事件名以决定放行格式，禁用后不访问数据库或暂存区。错误日志只用固定类别和说明，不格式化输入或底层错误正文；文件 0600、最近 200 行，写日志失败不改变放行。11 份合成 JSON 夹具通过后删除 `tools/cairn-probe`，同步 workspace 和锁文件。未改核心模块或表结构。

**验证**：保留真实 RED→GREEN：最初 hook CLI 未注册，测试因 InvalidSubcommand 失败；增加入口后 SessionStart 缺少 JSON 注入失败；子进程测试发现通用输出多了一个换行；禁用时名为 `unknown` 的未知事件与解析失败哨兵冲突，导致多输出 `{}`。各问题均在对应实现后通过。最终 `cargo test -p cairn --test hook` 的 7 项测试通过，覆盖 11 份夹具、未采用（库不存在及明确 unadopt）、禁用时损坏数据库与待收文件不动、两种 Stop 续跑/放行、JSON/核心/数据库忙/Git 超时失败放行、日志权限/保留/写入失败。隐私测试用唯一合成标记检查输出、数据库及侧文件、错误日志和暂存区；transcript_path 指向隔离目录中的 FIFO，正常完成证明未打开读取。

直接相关边角检查：重复 Stop；startup / resume / clear / compact / fork / 未知 source 映射；缺少回合 ID；未知事件（含禁用时）；hook 使用 JSON cwd 而非 `run_at` 的 cwd；SessionEnd 在暂存根不可用时仍记录结束且不收取；SessionEnd 在数据库锁冲突及模拟慢 Git 下小于 1.5 秒返回。已采用路径包含临时 Git 仓库；全部材料及环境均隔离。没有扩大到真实 agent 实测。

探针删除后，`CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target cargo test --all-targets` 跑一次通过（89 项）；`CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target cargo clippy --all-targets -- -D warnings` 跑一次通过。`cargo fmt --all`、`git diff --check` 通过。所有命令均在前台等待完成。

**拿主意的地方**：Codex Interrupt 直接忽略，不记数据库事件、不判定或续跑（任务允许选择）。未知 source，包括 fork，按任务映射为 other。JSON 已损坏到无法可靠识别事件时，Claude 静默，Codex 返回合法空对象 `{}`，避免潜在 Stop 得到非 JSON；有效未知事件始终静默。SessionEnd 的两次 Git discovery 各限 100 ms，保留核心既有 200 ms 数据库等待策略。一次失败日志只记录首个错误类别，写日志使用非阻塞锁，避免延长 hook 等待；状态路径本身不可用或日志不可写时尽力而为。生产沿用系统 DARWIN_USER_TEMP_DIR，测试用显式临时可信根。

**没做的事**：未实现 install / uninstall / status / README，未改 DESIGN、HANDOFF 或任何核心行为；未运行真实 agent，未读真实会话、工具记忆、真实 cairn 数据或 Corral 内部文件，未改用户配置。未合并 main、未推送。无须主控决定的事项，分支提交后交主控和独立审查者复核。

## 返工记录

2026-10-05，按交叉审查 R1 返工。将共享 Metadata 的 `stop_hook_active` 改为 `Option<bool>`，保留缺失与 false 的区别；仅在 Stop 分支要求该字段存在，缺失（含 null）时在调用 `turn_ended` 前返回 JSON 元数据错误。类型不正确仍由 serde 拒绝。两种情况均走既有错误放行路径，日志使用固定 `json` 类别和 `invalid hook JSON or metadata` 说明，不包含输入正文；其他事件不要求该字段，核心接口和行为未改。

新增 Claude / Codex 两项回归，均经隔离 `cli::run_at` 先采用项目、记录 UserPromptSubmit，再分别送缺字段和错误字符串类型的 Stop，核对输出为空串 / `{}`、`turn_decisions` 始终为 0、每次追加一行固定错误日志且不含合成隐私标记。修复前定向运行两项均 RED：实际错误返回 block；修复后两项均 GREEN。

验证范围仅为上述回归及指定检查，所有 Cargo 命令使用共用 `CARGO_TARGET_DIR` 并在前台等待结束：`cargo test -p cairn --test hook stop_requires_continuation_marker`（RED 2 失败，修复后 GREEN 2 通过）；`cargo test --all-targets` 跑一次通过（91 项）；`cargo clippy --all-targets -- -D warnings` 跑一次通过。未改核心模块、设计、真实配置或数据，主仓库交叉审查文件只读；仅提交到 `p3a-hook`，不合并、不推送。无新增待主控决定事项。

## 主控审查

2026-10-05，cairn/main。结论：通过，已合并。
- 初审：没动核心模块和 DESIGN；验收 4 条与隐私标记检查都有测试；探针已从 workspace 删除；测试与 clippy 通过；真实临时目录无 `cairn-spool`。
- 同意 Codex 的 Interrupt 只忽略、不记录不续跑。
- 交叉审查（`docs/tasks/P3a-hook适配-交叉审查.md`）：必须改 1 条（Stop 缺 `stop_hook_active` 时被当成 false 而误续跑），返工 2e55fbd 改为放行并记无正文日志，复核"可以合并"。
- 合并后 main 上全量测试与 clippy 通过。
