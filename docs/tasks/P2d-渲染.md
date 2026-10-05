# 任务：2d SessionStarted 核心流程、注入渲染与 cairn show

2026-10-05，cairn/main（主控）交给 cairn/dev-render（Codex，常规：gpt-6-astra / high）。
路由：常规 / 交叉审查要 / 影响面：碰要害（路由：档拿不准（重 0.57、常规 0.43）、要、碰要害；档按规则取常规）
类型：功能变更
依据：本轮做 SessionStarted 的核心函数（不含 hook 的 JSON 解析）、注入文本渲染、`cairn show`；不做回合判定（2e 并行在做）、其他用户命令（2f）、hook 入口与 install（阶段 3）。
提示：围绕已确认的使用目标完成变更，优先沿用现有机制。
你是被委派的 agent：照本文件做，不要再开别的 agent。

## 先读
- `AGENTS.md`（规矩一节）
- `docs/DESIGN.md` §3、§5、§6.2、§7（show 一行）、**§8.1**、§8.5（correct / retract / restore 的显示规则）、**§9 全文**、**§11 全文**、§12
- `docs/实施计划.md` 阶段 2 的 2d 一行
- 已合并的模块（直接用，不改已有行为）：`store.rs`、`scope.rs`、`facts.rs`（`compare` 生成现场对比行）、`spool.rs` / `ingest.rs`（收取）、`save.rs`（`StoredFacts`：持久化的事实要用这个类型读，不要反序列化成原始 `GitFacts`）、`cli.rs`
- `docs/tasks/P2c-adopt与save.md` 的完成记录（时间格式、接口约定）

## 在哪里干活
- worktree：`/Users/firegnu/Developer/personal_projs/cairn-worktrees/p2d-render`，分支 `p2d-render`（已从 main 建好）。
- 新建模块（例如 `render.rs`、`session.rs`），在 `lib.rs` 声明；在 `cli.rs` 加 `show` 子命令。测试放 `crates/cairn/tests/` 下以 `render` 或 `session` 开头的文件。
- 并行任务：cairn/dev-turn 在分支 `p2e-turn` 上写回合判定（`turn.rs` 之类）。你不要动它的文件；两边都会在 `lib.rs` 加一行 `mod`，冲突由主控合并时处理。
- 编译：所有 cargo 命令前加 `CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target`。

## 要做的
- **SessionStarted 核心函数**（§8.1）：入参是已解析好的值（是否禁用、agent、session_id、cwd、start_kind、当前时间），返回要注入的文本或"不输出"。依次：禁用 → 不输出；项目未采用 → 不输出、不收取；收取暂存区（当前来源优先）；登记来源（`claude:<sid>` / `codex:<sid>`，`association='hook'`，已存在则更新 `last_seen`）；写 `session_started` 事件；渲染；把注入过的记录写进 `injections`。start_kind 的区别按 §8.1 第 6 步（resume / fork 的规则已在 DESIGN 里定好）。
- **渲染**（§9.1 的六部分，顺序照写）：
  - 抬头用 §9.2 原文，把来源 ID 填进去；项目已采用但没有记录时只注入抬头。
  - "可见"的定义：没被删除（删除的显示墓碑一行或不显示，你定，写进完成记录）、没被取代、没被撤回；被 `restore` 撤销了取代 / 撤回的重新可见。有更正时，在原记录后附最新一条更正，标明来源和时间（§8.5）。
  - 本工作线：每个来源最新一条可见记录，新的在前，正文完整；每条附"之后观测到的事件"，按 §11.1（未确认回合的数法已在 DESIGN §11.1 写定；会话结束看 `events` 里的 `session_ended`；固定措辞和禁用说法照 §11.1）。
  - 折叠提示、现场对比（用 2b 的 `compare`）、其他工作线一行摘要（分支、多久之前、"停点"第一行；路径不存在写"已不可定位，只作历史"）、未显示的来源数和 `cairn list --line`。
  - 预算 6,000 字符（按字符数计，可配置常量）：不够时较早来源只留"停点"一节，再不够只列来源和查看命令（§9.1）。
  - "多久之前"用传入的当前时间算，保证测试可重复。
- **`cairn show [--json]`**：对当前目录输出和注入相同的内容（来源 ID 位置用占位文字），不写 `injections`、不登记来源；`--json` 输出结构化结果，字段你定。执行前按 §6.5 先收取。数据库不存在时报告"尚无数据"，不建库（用 2a 的只读打开）。

## 怎么算做完
以下验收条件经用户 2026-10-05 同意，取自实施计划 2d 的"建议验证"：
1. 固定样例比对：用合成数据构造一组典型情形（多来源、取代、撤回、更正、恢复、其他工作线、现场有变化），渲染结果与固定的期望文本逐字一致。
2. 预算边界：内容超过 6,000 字符时按 §9.1 的顺序截断，结果不超过预算。
3. 措辞红线：任何情形下输出里都不出现"崩溃""丢了""已推送"（以及 §11 列出的其他禁用说法）。
4. 记录 injections：SessionStarted 注入了哪些记录，`injections` 里就有哪些；`cairn show` 不写。

另外按任务要求：未采用 / 禁用时不输出、不收取；resume / fork 只补未注入过的记录。

验证只做这些：上面各条一个测试，再补你判断直接相关的边角用例（完成记录里列出）；`cargo test --all-targets` 和 `cargo clippy --all-targets -- -D warnings` 各跑一次并通过。测试只用临时 `XDG_STATE_HOME` / `HOME` / 临时 Git 仓库，暂存区可信根指向临时目录。觉得不够，在回复里说，不要自己加。

## 不要做
- 不要读写真实的 cairn 数据库、真实 `~/.local/state`、真实临时目录下的 `cairn-spool`；不要读真实会话记录、工具记忆。不用 Python。
- 不要改 store / schema / scope / facts / spool / ingest / save 的已有行为，不改表结构；需要它们加小接口时停下来报告。
- 不要做回合判定、TurnStarted / TurnEnded / SessionEnded、其他用户命令、hook 入口、install。
- DESIGN 说不通的地方停下来报告，等主控决定。
- 不要按项目名或路径批量杀进程。
- 不合并到 main，不推送。只在 `p2d-render` 分支上提交。

## 做完
在本文件末尾追加「## 完成记录」（在你的分支里提交）：做了什么、验证了什么、拿主意的地方、没做的事，各几句话。回复里只写这几样，加上有没有要主控决定的事。命令都在前台跑完，全部做完后，回复最后一行写 DONE。

## 完成记录

2026-10-05，cairn/dev-render（Codex），分支 `p2d-render`。

**做了什么**：新增 `render.rs`、`session.rs`，在 `lib.rs` 声明，并在 `cli.rs` 接入 `cairn show [--json]`。渲染按 §9.1 顺序输出抬头、本工作线各来源最新可见记录、折叠提示、现场对比、其他工作线摘要和未显示来源数。删除记录不显示；更正附最新可见一条并注明来源和时间；恢复撤销针对原记录的较早取代 / 撤回，之后的新取代 / 撤回仍生效。同时间用记录 ID 稳定排序。现场对比以本工作线最新可见且带事实的 checkpoint 为基线，通过 `StoredFacts` 读取持久化事实，再调用原有 `Git::compare`，没有改地基模块。

`session::start(&SessionStarted, database, root)` 接收已经解析的禁用状态、agent、session_id、cwd、start_kind 和当前时间；数据库和可信暂存根显式传入。禁用先退出；库不存在 / 项目未采用时不建库、不收取、不登记；通过采用检查后按当前来源优先收取，再在同一个 IMMEDIATE 事务内登记来源、写 `session_started`、渲染和写 `injections`。新来源为 hook 关联，已有来源只更新 `last_seen`；注入失败回滚这一事务，先前独立提交的暂存收取不回退。错误返回调用者，未来阶段 3 hook 入口负责按 §8.6 记日志并放行，不在本任务实现适配器。

startup / clear / compact / other 完整渲染；resume / fork 只补本来源尚未注入的内容与现场对比，不重复折叠提示和其他工作线。原记录已注入之后的新更正，注明目标 ID 单独补入，不重复原正文。抬头按 §9.2 原文填来源 ID，空项目只返回抬头。`show` 使用同一渲染函数，来源位置为 `<本来源ID>`；已有库先收取，再在读取事务中渲染，不登记调用者、不写 `injections`。无库先经 `Store::open_read_only` 返回“尚无数据”，不打开暂存区、不建库；JSON 无库为 `{"status":"no_data"}`，有库为 `text`、`record_ids`、`omitted_sources`。

**验证了什么**：新增 `crates/cairn/tests/render_session.rs`，共 10 项测试，覆盖四条验收及禁用 / 未采用、resume / fork 要求。有效 RED→GREEN 包括：原 CLI 不识别 show；固定样例缺少渲染；输出超出预算；采用项目没有注入和登记；较新的隐藏记录未标出最后落盘时间；resume 漏掉后来的更正。实现过程中一次 Rust 类型错误、一次夹具真实时间早于合成记录时间、一次预算夹具的容量估算错误，不计作 RED。

直接相关边角验证：

- 固定期望文本逐字比对多来源、同来源旧记录、取代与恢复、撤回与恢复、删除隐藏、最新更正、缺口统计、会话结束、纯 CLI 来源、其他工作线不存在、Git 工作区新增文件；时间来自固定入参。
- 中文按 `chars().count()` 计预算、正好等于预算、较早来源完整正文→停点→查看引用、引用也放不下时的来源计数。长停点仍超预算时只留引用，未显示正文的记录不写入 injections，下一次 resume 仍能补入。
- 禁用在 cwd / 数据库 / 暂存区操作之前生效；未采用时不收取；收取发生在新会话来源登记之前，原来未知的声明来源按现有 2c 规则落为 local 来源。
- show JSON 字段与注入相同记录，show 会收取暂存正文，但不额外登记来源 / 事件 / injections；无库的普通和 JSON show 不建目录。
- resume / fork 去重，fork 使用新来源 ID 时只按自己的 injections 判定；clear / compact / other 完整补入；空项目精确抬头；既有来源 first_seen 保留、last_seen 更新。
- 合成 SQLite 触发器使第二条 injections 写入失败，检查来源 / 事件 / injections 无部分提交；移除故障后成功，预算引用仍不记为正文注入。
- 事件统计仅数最后一个 checkpoint 之后的 `unconfirmed_after_continue`，不把 `pending_unprocessed` 算作未确认；最新 checkpoint 被撤回时仍指出真实最后落盘 ID / 时间。
- 自动文字遵守措辞红线，历史正文内出现相同词语仍原样保留（见下方用户确认）。

所有命令均在前台等待完成，Cargo 使用指定共享编译目录。`cargo test --all-targets` 一次通过：新增渲染 / 会话 10、原 save 19、scope/facts 5、store 17、probe 4，共 55 项。`cargo clippy --all-targets -- -D warnings` 首次指出测试断言不必要的 `clone`，按建议改为 `std::slice::from_ref`，重跑通过；没有放宽断言或 lint。格式化和 `git diff --check` 通过。测试全部使用合成材料、临时 HOME / XDG_STATE_HOME、临时 Git 仓库和显式临时暂存可信根，没有在真实暂存根建 cairn-spool。

**拿主意的地方**：删除记录完全隐藏，不输出墓碑。预算默认常量 6,000，库级渲染可传预算；先从旧到新将正文缩为停点，再从旧到新缩为引用；仍放不下时省略较早的其他线摘要、折叠提示和来源引用，保留未显示的本工作线来源数及 `cairn list --line`。不会切断 UTF-8、抬头规则或现场对比；连固定内容也容不下的自定义预算返回错误。只有实际带内容输出的 checkpoint、更正和其他线摘要进入 `record_ids` / injections；折叠提示和纯查看引用不算注入。记录时间统一沿用 UTC RFC 3339 毫秒格式，过去时长用传入时钟计算，未来时间显示“不到 1 分钟前”。

用户在本轮明确确认措辞范围：**“限定自动生成文字，正文原样保留”**。因此禁用词检查针对 cairn 自动叙述，不审改历史正文，也不把历史中的模型判断变成程序事实。未修改 DESIGN 的既定行为。

**没做的事**：未实现回合判定、其他用户命令、hook JSON 入口、错误日志入口或 install；没有修改 store / schema / scope / facts / spool / ingest / save、HANDOFF、真实配置，未访问真实 cairn 数据或真实会话。未启动 / 委派 agent，未合并 main、未推送。查看 / 恢复提示中的 `show <ID>`、`list --line`、`restore` 命令由 2f 接续实现。没有需要主控决定的事项；交叉审查与合并留给主控。

## 返工记录

2026-10-05，按交叉审查第 1 条（必须改）和第 2 条（建议改）返工。主仓库 `docs/tasks/P2d-渲染-交叉审查.md` 全文只读查看，未修改。

**修改**：其他工作线摘要沿用本工作线的最新可见更正筛选条件，在原摘要后附上该更正的完整正文、来源和时间，并将更正 ID 加入同一输出片段的 `record_ids`。原摘要与更正一起参与已有字符预算：一起展示并登记 injections，或因预算不足一起省略，不留下缺少更正的旧说法。仅一行摘要的提取改为寻找停点中的首个非空内容行，跳过开头空行及仅含空白的行；完整正文和停点一节的渲染保持原样。用户已确认的“限定自动生成文字，正文原样保留”边界不变。

**验证**：仅新增并定点运行两个测试，均先确认目标断言失败，再实施修复并转为 GREEN。`other_line_summary_injects_latest_visible_correction_within_budget` 使用临时 Git 仓库、其他历史工作线的“旧说法：接口已验证”和“更正：接口尚未验证”，通过公开 SessionStarted startup 入口复现原先遗漏更正；修复后核对最新可见更正的正文、来源、时间、record_ids 和数据库 injections，同时排除较早和已撤回的更正，并验证缩小预算时原摘要与更正一起省略。`other_line_summary_skips_leading_blank_lines_without_changing_full_body` 复现标题后空行导致摘要为空，修复后逐字核对首个非空行摘要，并确认相同材料在本工作线的完整正文不变。

随后 `cargo test --all-targets` 和 `cargo clippy --all-targets -- -D warnings` 各运行一次，均通过；全量共 57 项（render/session 12、save 19、scope/facts 5、store 17、probe 4）。所有命令均前台等待结束，Cargo 使用指定共享 target；测试仍为合成材料、临时 HOME / XDG_STATE_HOME、临时 Git 仓库和显式临时暂存可信根。格式化和差异空白检查通过，没有扩大验证范围、放宽断言或改超时。

**范围**：本次仅修改 `render.rs`、`tests/render_session.rs` 和本任务文件，提交到 `p2d-render`；未改 DESIGN、地基模块、主仓库审查文件、真实配置或数据，未合并、未推送。没有新增需主控决定的事项。
