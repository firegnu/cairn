# 交叉审查：3a hook 适配

2026-10-05，cairn/main（主控）交给 cairn/dev-review-hook（Codex，重：gpt-6-astra / xhigh）。
类型：调研
提示：区分已查证事实、推测和未知，给出直接依据；结论限定在本轮调查范围。
你是被委派的审查者：只读审查，不要开别的 agent。**本文件在主仓库工作区（`/Users/firegnu/Developer/personal_projs/cairn/docs/tasks/`），不在你的审查 worktree 里；你只写本文件这一个文件。**

## 背景
- cairn/dev-hook（Codex）在分支 `p3a-hook` 上实现了 `cairn hook claude|codex` 并删除了探针，提交 `cd2a8ed`。任务书 `docs/tasks/P3a-hook适配.md`（审查 worktree 里也有，末尾有完成记录）。
- 主控已审过：没动核心模块和 DESIGN；探针已从 workspace 删除；重跑 `cargo test --all-targets`（89 项）、clippy、`git diff --check` 通过；真实临时目录下无 `cairn-spool`。

## 先读
- `AGENTS.md`（规矩一节，尤其隐私）；任务书与完成记录；任务书"先读"里列的 DESIGN 章节和第一阶段实测报告章节。

## 要审查的
- 审查 worktree：`/Users/firegnu/Developer/personal_projs/cairn-worktrees/review-p3a-hook`（detached，指向 `cd2a8ed`）。改动范围：`git diff main...HEAD`。**只读：不改、不提交、不切分支。**
- 可以跑该分支的测试和 clippy（命令前加 `CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target`），可以用临时 `XDG_STATE_HOME` / `HOME` 直接调用编译出的 `cairn hook` 喂合成 JSON；核实具体怀疑时可在临时目录写一次性程序，不留在 worktree 里；暂存区可信根一律指向临时目录。
- 不要运行 claude、codex，不要开 agent；不要读真实 `~/.local/state`、真实会话记录、工具记忆、Corral 事件文件；不要写任何真实配置。不用 Python。只做定点核实，不做覆盖矩阵。

## 重点看
1. **输出格式与两种工具的契约**（DESIGN §10.1 / §10.2、实测报告）：SessionStart 注入 JSON、Stop 续跑 / 放行（Claude 不输出、Codex `{}`）、其他事件不输出；有没有任何路径输出纯文本或退出码非 0（尤其 Codex Stop 不接受纯文本）。
2. **隐私**：`prompt`、`last_assistant_message` 不进数据库、`errors.log`、stdout / stderr；`transcript_path` 不被打开；JSON 解析错误信息会不会把原文带进日志。
3. **失败策略**（§8.6）：坏 JSON、缺字段、数据库忙、git 超时、核心函数出错时都退出 0 并按事件放行；`errors.log` 有界、0600、写失败不影响退出码；有没有可能 panic。
4. **分派与字段映射**：turn_key 取 `prompt_id` / `turn_id`、`stop_hook_active` → continued、`source` → start_kind、来源 ID、cwd；`CAIRN_DISABLE` 早退且不碰数据库；未采用项目不写库。
5. **时限**：SessionEnd 是否只写事件、不收取不渲染；其他事件的最坏耗时是否受 2c / 2e 的预算约束。

## 输出
追加到本文件末尾「## 审查意见」：先写一句结论（**可以合并** / **改完再合并**）；每条意见写级别（必须改 / 建议改 / 可以不改）、位置、问题、改法。**"必须改"只给两种情况**：任务书"怎么算做完"没达到；或者碰到的要害上有真实缺陷（隐私泄露、输出格式让 agent 出错或误续跑、出错不放行）。其余一律"建议改"。能给复现步骤的写上。最后对完成记录里"拿主意的地方"逐条表态。

## 回复
回复里只写结论和各级别条数。命令都在前台跑完，全部做完后，回复最后一行写 DONE。

## 审查意见

**改完再合并。**

2026-10-05，独立审查固定提交 `cd2a8ed85e7dbbd8f4f4e28abcdf21478c037764`，范围为审查 worktree 的 `git diff main...HEAD` 及直接调用的核心接口。发现 1 项必须改；以下结论不外推到真实 agent 或未测的宿主版本。

### R1 — 必须改：Stop 缺少续跑标记时仍请求续跑

- **位置**：`crates/cairn/src/hook.rs:56–57`、`:179–180`；现有缺字段测试在 `crates/cairn/tests/hook.rs:480–488`，只覆盖缺少回合 ID。
- **问题（已查证）**：`#[serde(default)] stop_hook_active: bool` 把缺字段变成 `false`。已采用项目收到带有效来源和回合 ID、但缺少 `stop_hook_active` 的 Stop 时，适配器仍调用正常结束判定；没有确认时，两种工具均收到 `{"decision":"block","reason":"…"}`，且数据库写入 `continue_requested`，没有错误日志。这是在不能确认是否已经续跑时主动请求续跑，违反本任务对缺字段错误放行的要求，属于要害上的真实误续跑缺陷。这里没有声称本机正常版本已观测到缺字段；复现的是适配器收到不完整输入时的实际行为。
- **复现**：在 `/private/tmp/cairn-p3a-review.EJAZHr/check.rs` 写一次性 Rust 程序，链接本 worktree 构建的 `libcairn.rlib`，临时 HOME / XDG / Git 配置和显式暂存可信根均隔离。先经 `cli::run_at` adopt 临时非 Git 项目，再分别发送 `UserPromptSubmit(session_id=synthetic-review-claude|codex, prompt_id|turn_id=synthetic-turn)`，随后发送同来源、同回合的 Stop，唯独省略 `stop_hook_active`。两次实际输出均为上述 block JSON；查询 `turn_decisions` 得到 `continue_requested_count=2`，`error_log_exists=false`。再送 `stop_hook_active=true` 时，Claude 输出空串，Codex 输出 `{}`。程序在前台运行完成；未改仓库代码或测试。
- **改法**：保留“字段缺失”与 `false` 的区别，只对 Stop 要求存在且类型正确的 `stop_hook_active`；缺失时进入已有固定类别日志及按工具放行的错误路径，不调用 `turn_ended`。可用 `Option<bool>` 后在 Stop 分支验证，或按事件分别解码。不要直接把共享 Metadata 的该字段改成所有事件必填，否则正常 SessionStart / UserPromptSubmit / SessionEnd 也会失败。补一个两种工具缺少该字段的回归检查：输出分别为空串 / `{}`，没有新增续跑决定，并记录不含输入正文的错误。

### 本轮核实结果

- 重新运行 `cargo test -p cairn --test hook`：7 项通过；再运行 `cargo test --all-targets`：89 项通过；`cargo clippy --all-targets -- -D warnings`、`git diff --check` 通过。所有 Cargo 命令使用规定的共用 target，所有命令均在前台等到结束。现有绿测没有覆盖 R1。
- 输出和隐私：`hook.rs:43–59` 选择性解码元数据，隐私字段被忽略；`:195–225`、`:273–293` 使用固定类别、白名单事件名和固定说明，不格式化 serde 或底层错误正文。正常路径只生成任务规定的 JSON / 空输出，二进制专用入口不附加用户命令换行。合成标记、transcript FIFO、错误类型和日志写入失败检查均通过；本轮未发现隐私字段落盘或读取 transcript 的路径。
- 分派和失败：来源、JSON cwd、两种回合 ID、启动类型、已提供的 continued 标记均映射到既有接口；禁用在状态路径及暂存区解析前返回，未知事件及 Interrupt 静默返回。未采用、坏 JSON、数据库锁冲突、Git 超时和已覆盖的核心错误均按事件放行。除 R1 外，本轮未发现可由所审输入触发的 panic 或非零退出路径。
- 时限：SessionEnd 只做作用域解析与 `turn::session_ended`，不收取、不渲染；慢 Git 与数据库锁检查均在 1.5 秒内完成。其余事件沿用既有收取 50 个文件 / 300 ms、Stop 待收查询 100 ms、数据库 Hook 等待 200 ms 和普通 Git 单命令 2 秒预算。它们是分段预算，不能把 300 ms 当作整个 hook 的耗时上限；本轮没有新增整条链路的硬实时保证。
- 差异确认：核心模块、表结构和 DESIGN 未改；探针 workspace 成员、源码、测试和锁文件条目已删除。审查未运行真实 agent、未接触真实状态库、会话、工具记忆或真实配置；未提交、未切分支。

### 对完成记录「拿主意的地方」逐条表态

1. **可以不改 — Codex Interrupt 不记事件**。位置：`hook.rs:91–95`。任务明确允许选择是否记观测；忽略后不判定、不续跑、不写库，与允许的选择一致。无需修改。
2. **可以不改 — fork 和未知 source 映射 other**。位置：`hook.rs:129–135`。本任务明确要求 startup / resume / clear / compact 以外记 other，现有测试也核实了 fork / future。按本任务保留，不在审查中重新决定 fork 策略。
3. **可以不改 — 无法识别事件的坏 JSON 使用保守放行格式**。位置：`hook.rs:68–73`、`:81–89`、`:108–116`。Claude 空输出、Codex `{}` 不会请求续跑；有效未知事件仍静默。已覆盖的解析错误不会将原文写入日志。无需修改。
4. **可以不改 — SessionEnd 的 Git 单次 100 ms 及核心 200 ms 数据库等待**。位置：`hook.rs:156–163`、`turn.rs:247–263`、`store.rs:73–78`。短 Git 预算为结束事件写入留出时间，超时放行；没有新增收取或渲染。已覆盖的慢 Git / 锁冲突检查通过，无需修改。
5. **可以不改 — 只记首个错误、日志非阻塞锁及不可写时尽力而为**。位置：`hook.rs:184–185`、`:228–295`。首个错误足以留下本次失败类别；锁竞争或日志写入失败不改变放行。0600、最近 200 行和写失败检查通过；非阻塞锁是代码核实，未另做并发压力实验。无需修改。
6. **可以不改 — 生产使用 DARWIN_USER_TEMP_DIR，测试显式临时可信根**。位置：`hook.rs:98–105`、`spool.rs:35–56`。复用已有生产根选择规则；`run_at` 提供隔离路径，本次定点复现也使用该入口。无需增加生产环境测试开关或修改根选择规则。

条数：必须改 1，建议改 0，可以不改 6（六项为上述实施取舍的逐条判断）。

## 复核意见

**可以合并。**

2026-10-05，复核固定提交 `2e55fbd731bc6d308ff11f232245fcbb45307cc2`；仅检查 R1 是否关闭及 `git diff cd2a8ed..2e55fbd` 的直接影响，没有重新进行全量审查。

- **R1 已修复**。位置：`crates/cairn/src/hook.rs:56`、`:179–183`。`Option<bool>` 保留缺失与 `false` 的区别，仅 Stop 分支在调用 `turn_ended` 前要求标记存在；缺失进入已有 JSON 错误放行路径，错误类型仍由 serde 拒绝。其他事件没有新增必填要求，显式 `false` / `true` 的核心调用语义不变。`null` 解码为 `None` 后同样会被 Stop 分支拒绝，这是代码核实，未另加专项测试。
- **独立复现通过**。将首轮原样的 `/private/tmp/cairn-p3a-review.EJAZHr/check.rs` 重新链接到本提交构建的库，在全新 `/private/tmp/cairn-p3a-r1-review.JCQKwa` 隔离状态、HOME / XDG / Git 配置和暂存可信根后运行。缺少标记的 Stop，Claude 实际输出空串，Codex 输出 `{}`；`continue_requested_count=0`；日志分别追加 `claude Stop json invalid hook JSON or metadata` 和 `codex Stop json invalid hook JSON or metadata`。此前的误续跑已不再出现。
- **差异未发现新问题**。改动仅含上述元数据类型和 Stop 校验、两种工具的回归检查及返工记录。新增测试位于 `crates/cairn/tests/hook.rs:230–274`，核对缺字段和错误字符串类型的放行输出、零回合决定、固定错误日志及合成隐私标记不泄漏；原有测试继续覆盖其他事件缺少该字段、正常续跑、continued 放行、禁用和未采用路径。未改核心模块、公共输出格式或原有断言。
- **本轮验证**：`CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target cargo test -p cairn --test hook`，9 项通过；同一 target 下 `cargo build -p cairn --lib`、`cargo clippy --all-targets -- -D warnings` 通过；`git diff --check cd2a8ed..2e55fbd` 通过。仅重跑与修复直接相关的测试，没有重复全量测试；返工记录中的 91 项全量通过是开发方记录，不作为本轮亲自重跑的结果。所有命令均在前台完成。

R1 关闭 1 项；本轮新增意见：必须改 0，建议改 0，可以不改 0。前轮六项实施取舍未重开。审查 worktree 保持干净且仍为上述 detached 提交；仅向本审查文件追加复核结果，未改代码、测试、真实配置或数据，未提交、未切分支。
