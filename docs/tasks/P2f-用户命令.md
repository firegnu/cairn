# 任务：2f 用户命令（list、show <ID>、correct、retract、restore、delete、export）

2026-10-05，cairn/main（主控）交给 cairn/dev-cmds（Codex，重：gpt-6-astra / xhigh）。
路由：重 / 交叉审查要 / 影响面：碰要害（路由：档拿不准（重 0.83、常规 0.17）、要、碰要害；delete 涉及隐私正文清除，主控取重）
类型：功能变更
依据：本轮补齐 DESIGN §7 里给用户用的记录管理命令；不做 install / uninstall / status（阶段 3）、hook 入口（阶段 3）。
提示：围绕已确认的使用目标完成变更，优先沿用现有机制。
你是被委派的 agent：照本文件做，不要再开别的 agent。

## 先读
- `AGENTS.md`（规矩一节）
- `docs/DESIGN.md` §3（更正 / 撤回 / 恢复 / 删除 / 取代）、§5（来源）、§6.2、§6.5（"用户命令执行之前也收取"）、**§7**、**§8.5**、§9.1（可见性规则）、§12
- `docs/实施计划.md` 阶段 2 的 2f 一行
- 已合并的模块（直接用，不改已有行为）：`store.rs`（`delete_body`：返回 `CheckpointBusy` 时墓碑已提交、物理清除未完成）、`cli.rs`、`render.rs` / `session.rs`（可见性与更正显示的现有实现，尽量复用，别另写一套规则）、`spool.rs` / `ingest.rs`、`save.rs`（`StoredFacts`）
- `docs/tasks/P2d-渲染.md` 的完成记录（渲染里的可见性取舍，例如删除记录不显示）

## 在哪里干活
- worktree：`/Users/firegnu/Developer/personal_projs/cairn-worktrees/p2f-cmds`，分支 `p2f-cmds`（已从 main 建好）。
- 可以新建模块（例如 `commands.rs`），在 `lib.rs` 声明；在 `cli.rs` 加子命令。`show` 已有"无参数 = 输出注入内容"，改成 `show [ID] [--json]`：带 ID 时显示单条记录，不带时保持现有行为。测试放 `crates/cairn/tests/` 下以 `cmds` 开头的文件。
- 只在可见性 / 更正显示的现有函数需要小幅抽出复用时改 `render.rs`，不改它的输出。不要改 store / schema / scope / facts / spool / ingest / save / turn / session 的已有行为。
- 编译：所有 cargo 命令前加 `CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target`。

## 要做的
所有命令在用户自己的 shell 里运行：执行前先收取暂存区（§6.5），直接用 `Store::open`（用户命令档的 busy timeout）读写数据库；只读命令（list、show、export）在库不存在时报告"尚无数据"，不建库。出错返回非 0，stderr 一行说明（§7）。写入的记录照"写入后不改、只追加"（§6.2 约束），来源按 §5：用户命令没有会话 ID，用 `local:<ULID>`，关联不确定。时间格式沿用 RFC 3339 UTC 毫秒。

- **`list [--line] [--all]`**：列出当前项目（`--line` 只看当前工作线）的记录：ID、时间、来源、工作线 / 分支、kind、"停点"第一行；默认隐藏被取代、被撤回、已删除的，`--all` 全部列出并标明状态。
- **`show <ID> [--json]`**：显示一条记录的元数据、事实、正文；有更正时附最新一条更正（来源、时间，§8.5）；被取代 / 撤回 / 删除时标明状态；删除的只显示墓碑元数据。
- **`correct <ID>`**：正文从 stdin，校验规则同 save（非空、有"停点"、不超 6 KiB）；插入 `kind=correction`、`target_id=<ID>`。目标不存在或已删除时拒绝。
- **`retract <ID>`**：插入 `kind=retraction`。被撤回的默认不显示，`list --all` 能看到。
- **`restore <ID>`**：插入 `kind=restore`，撤销对 `<ID>` 的取代或撤回，使它重新可见；没有可撤销的就报错不写。
- **`delete <ID> [--yes]`**：没有 `--yes` 时交互确认（stdin 不是终端且没 `--yes` 就拒绝）；调用 `delete_body`；输出被删记录剩下的元数据。`CheckpointBusy` 时明确报告"墓碑已写入、物理清除未完成，请稍后重试同一命令"，返回非 0，不能报成功。
- **`export [PATH]`**：抬头写"导出自 cairn，时间…，非权威"，内容是当前工作线各来源可见的记录（Markdown）。不带 PATH 输出到 stdout；带 PATH 时只新建文件（独占创建、0600），已存在就拒绝。

## 怎么算做完
以下验收条件经用户 2026-10-05 同意，取自实施计划 2f 的"建议验证"：
1. 更正后显示带来源：correct 之后，`show <ID>` 和注入渲染里都附上更正，标明来源和时间。
2. 撤回后默认隐藏，`--all` 可见。
3. 恢复：被取代或被撤回的记录 restore 之后重新可见。
4. 删除后只剩墓碑：delete 之后正文为空、元数据还在，数据库和 WAL 里找不到原正文；检查点忙时报告未完成且返回非 0。
5. export 不覆盖已有文件。

验证只做这些：上面各条一个测试，再补你判断直接相关的边角用例（完成记录里列出）；`cargo test --all-targets` 和 `cargo clippy --all-targets -- -D warnings` 各跑一次并通过。测试只用临时 `XDG_STATE_HOME` / `HOME` / 临时 Git 仓库，暂存区可信根指向临时目录。觉得不够，在回复里说，不要自己加。

## 不要做
- 不要读写真实的 cairn 数据库、真实 `~/.local/state`、真实临时目录下的 `cairn-spool`；不要读真实会话记录、工具记忆。不用 Python。
- 不要做 install / uninstall / status、hook 入口。
- 不要改表结构，不要改上面列出的已有模块行为；需要小接口时停下来报告。DESIGN 说不通的地方停下来报告，等主控决定。
- 不要按项目名或路径批量杀进程。
- 不合并到 main，不推送。只在 `p2f-cmds` 分支上提交。

## 做完
在本文件末尾追加「## 完成记录」（在你的分支里提交）：做了什么、验证了什么、拿主意的地方、没做的事，各几句话。回复里只写这几样，加上有没有要主控决定的事。命令都在前台跑完，全部做完后，回复最后一行写 DONE。

## 完成记录

2026-10-05，cairn/dev-cmds（Codex），分支 `p2f-cmds`。

**做了什么**：新增 `commands.rs`，接入 list、show [ID] [--json]、correct、retract、restore、delete、export。用户命令在执行前沿用已有收取入口，以 `BusyTimeout::UserCommand` 打开数据库；只读命令无库时返回“尚无数据”，不创建数据库、暂存区或导出文件。更正复用 save 的正文校验；更正、撤回、恢复在 IMMEDIATE 事务中校验目标并追加记录与新的 uncertain / local 来源；没有可恢复状态时不写入。删除调用原有 `delete_body`，检查点忙时返回指定的未完成错误，释放阻塞后可重试同一 ID；导出只独占新建文件，权限 0600。

`render.rs` 仅抽出项目记录查询、可见性、最新更正与停点提取供命令复用，附带读取项目 ID 和删除时间，没有修改原有可见性规则或注入文本。`show <ID>` 的 JSON 包含记录元数据、来源关联、状态、StoredFacts、正文和最新可见更正；墓碑的正文、事实和更正均为 null，文字版仅显示墓碑元数据。原 `show` 不带 ID 的渲染与 JSON 结构保留。另将 `main.rs` 的结果输出小幅抽至 `cli::report`：成功输出保留 Markdown 换行，错误仍为 stderr 一行、非 0 退出，修复原入口将所有成功输出压成一行的问题。

**验证了什么**：新增 `crates/cairn/tests/cmds.rs`，五条验收各有一项测试，另有四项直接相关边角测试和一个隔离子进程驱动，共 10 项。每条验收先在 CLI 不支持相应命令时取得有效 RED，再实现至 GREEN；Markdown 输出测试先复现成功输出被压成一行，再修复至 GREEN。删除验收使用独立 SQLite 读事务阻塞检查点，通过共用 CLI 输出入口的隔离子进程核对非 0 退出及精确单行错误；检查墓碑已提交，释放读事务、重试后扫描数据库与 WAL，确认原正文标记不存在、WAL 已截断、首次删除时间不变。

直接相关边角用例：

- 更正保留原正文，最新更正显示来源及 UTC RFC 3339 毫秒时间；停点摘要跳过标题后的空行；恢复后更晚的撤回仍生效，重复恢复拒绝且不追加。
- 空白、缺停点、超过 6 KiB、非法 UTF-8 的更正拒绝；恰好 6 KiB 接受；不存在或已删除目标拒绝，失败不新增记录；删除后的 show 不再附更正。
- list / show <ID> / export 无库时不创建任何状态；临时 Git 仓库及 worktree 验证项目和工作线筛选、分支与 StoredFacts 显示，以及从其他目录按 ID 更正仍归属目标项目和工作线。
- 非终端即使输入 yes，未带 --yes 仍拒绝删除；真实伪终端 stdin 下分别验证取消与确认；show / export 的进程输出保留 Markdown 换行。子进程使用 `run_at` 显式传临时可信根，再走与 main 共用的 `report`，不调用会选中系统真实暂存根的 `run`。
- export 保留当前线各来源的可见历史及最新更正，排除撤回内容和其他工作线；已有普通文件与符号链接均拒绝覆盖，新文件权限 0600，正文与 stdout 导出一致。

所有命令均在前台等待结束，Cargo 使用指定共享 target。`cargo test --all-targets` 和 `cargo clippy --all-targets -- -D warnings` 各运行一次、均通过：cmds 10、render/session 12、save 19、scope/facts 5、store 17、turn 16、probe 4，共 83 项；原渲染固定文本逐字比对通过。格式化及差异空白检查通过，没有改产品预算、放宽断言或修改已有测试。测试只使用合成材料、临时 HOME / XDG_STATE_HOME、临时 Git 仓库及显式临时暂存可信根。

**拿主意的地方**：ID 全局唯一，按 ID 的查看和管理不受当前项目限制，方便查询旧项目历史；追加声明沿用目标的项目、工作线和分支，不采集另一份 Git 事实，原事实不变。list 列出未被隐藏的各 kind，--all 另显示隐藏记录及其状态；export 导出当前线全部可见 checkpoint，附各自最新可见更正，不采用注入的每来源只留最新一条或字符预算。删除只处理指定记录的正文，不级联删除其他历史记录；墓碑展示不附正文、事实或更正。

**没做的事**：未修改 DESIGN、表结构和 store / scope / facts / spool / ingest / save / turn / session 的行为，未实现阶段 3 的命令或 hook；未访问真实 cairn 数据库、真实会话或真实暂存区，未改真实配置。未启动或委派其他 agent，未改 HANDOFF，未合并 main、未推送。没有需要主控决定的事项；交叉审查、合并和主控收尾留给主控。

## 返工记录

2026-10-05，按交叉审查两条“必须改”和主控决定返工。已只读查看主仓库 `docs/tasks/P2f-用户命令-交叉审查.md` 全文；先执行 `git merge main`，合并提交 `6228d48` 带入设计提交 `afd2212`，采用 DESIGN §8.5 的 checkpoint 更正边界。

**修改**：`commands.rs` 在追加事务中拒绝以 correction / retraction / restore 为目标的 correct，返回“correct 只允许以 checkpoint 为目标”，不创建来源或记录。对原 checkpoint 再次更正仍可成功，详情、注入及导出沿用最新更正显示。

restore 的排序不再只依赖当前墙钟：在 IMMEDIATE 事务中读取目标的已提交取代、撤回及恢复动作的最大 `created_at`，新时间取 `max(当前时间, 最大动作时间 + 1 ms)`。同一规则应用于后续 retraction，避免抬高时间的 restore 抵消用户之后的撤回。只追加新行，不改历史；仍使用原有 UTC RFC 3339 毫秒格式和 render 的 `(created_at, id)` 排序。原时间格式无效、时间无法递增或递增后超出规范格式时返回错误并回滚，不能留下无效恢复或孤立来源。

**验证**：只新增并定点运行两个回归，各自先 RED 再 GREEN：

- `correct_rejects_non_checkpoint_targets_without_writing`：先复现二级更正返回 0；修复后验证三种不支持的目标均非 0、stderr 精确一行、记录和来源不增加，并验证对原 checkpoint 重写更正仍进入详情、注入和导出。
- `restore_orders_after_committed_actions_even_when_wall_clock_is_behind`：仅在合成库中让已提交的取代、撤回时间领先当前墙钟，先复现 restore 返回成功但 `retracted=true`；修复后核对取代和撤回均解除、恢复时间晚于最大动作时间 1 ms、旧记录不变、列表和注入重新可见；同一回归还核对紧接着的撤回与再次恢复仍生效，以及无法表示下一个毫秒时非 0、单行错误且不追加记录或来源。没有改系统时钟，也没有用 sleep 消除倒序。

随后 `cargo test --all-targets` 与 `cargo clippy --all-targets -- -D warnings` 各运行一次，均通过。全量共 85 项：cmds 12、render/session 12、save 19、scope/facts 5、store 17、turn 16、probe 4。所有命令均前台等待完成，Cargo 使用指定共享 target；测试继续使用临时 HOME / XDG_STATE_HOME 和显式临时暂存可信根，没有新增其他测试或扩大验证范围。

**范围**：本次返工仅修改 `commands.rs`、`tests/cmds.rs` 和本任务文件；DESIGN 变更来自合并 main。未改 render 排序、禁改模块、真实配置或数据，未写主仓库审查文件，未委派 agent、未合并回 main、未推送。没有需要主控另作决定的事项。
