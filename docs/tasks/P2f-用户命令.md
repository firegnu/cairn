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
