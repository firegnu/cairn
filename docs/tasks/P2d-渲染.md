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
