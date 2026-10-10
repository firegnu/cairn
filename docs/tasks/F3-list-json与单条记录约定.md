# 任务：`cairn list --json`；把它和 `cairn show <ID> --json` 写成公开约定

2026-10-10，cairn/main 交给 cairn/dev-list-json（Codex，常规档：gpt-6-astra / high）。
路由：常规 / 交叉审查不要 / 影响面：改行为（路由：常规、拿不准、拿不准；后两项主控定：只读命令加输出，不动表结构和写入路径）
类型：功能变更
依据：paddock 的 Cairn 标签要改成一条条记录的列表、点开看单条（paddock P5-79），需要 cairn 给出记录列表和单条记录的 JSON。用户 10-10 对 paddock 主控的安排（cairn 这一半交给 cairn 主控）：“都按你的建议来，写任务文件吧”。本轮只加 `list --json`、定约定、升版本号；不改表结构，不改给人看的文字输出。
提示：围绕已确认的使用目标完成变更，优先沿用现有机制。
你是被委派的 agent：照本文件做，不要再开别的 agent。

## 先读
- `AGENTS.md`（规矩全文）。
- `docs/DESIGN.md` §7（命令清单）、§7.1（公开约定，“只加不改”）、§6.2 末尾“版本 2”里只读打开的说法。
- 代码：`crates/cairn/src/cli.rs` 的 `Show`、`List` 两个分支和 `existing_store`；`crates/cairn/src/commands.rs` 的 `details`、`show`、`list`；`crates/cairn/src/status.rs`（只读打开、版本 1 的库照样能读的现成做法）；`crates/cairn/src/render.rs` 的 `Record`、`records`。
- 测试：`crates/cairn/tests/cmds.rs`。

## 在哪里干活
- worktree：`/Users/firegnu/Developer/personal_projs/cairn-worktrees/f3-list-json`，分支 `f3-list-json`（已从 main 建好）。
- 编译：cargo 命令前加 `CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target`。
- 只动 `crates/cairn/src/cli.rs`、`crates/cairn/src/commands.rs`、`crates/cairn/tests/cmds.rs`、`crates/cairn/Cargo.toml`、`Cargo.lock`、`docs/DESIGN.md`、`README.md`、`README.zh-CN.md` 和本文件；别的文件确实要动，在完成记录里写明为什么。

## 要做的

字段名、类型、含义照下面写的做，paddock 那边照同一份约定写。

### 1. `cairn list --json [--limit N]`

- `--json` 可以和现有的 `--line`、`--all` 一起用；`--limit N`（非负整数）只能和 `--json` 一起用。
- **只读**，和 `cairn status` 一样：只读方式打开数据库，不收取暂存区，不创建、不升级数据库；版本 1 的库照样能读。所以刚 `save`、还没收取的记录不在结果里。
- 不带 `--json` 的 `cairn list` 行为和输出都不变（照旧先收取暂存区）。
- 退出码 0，stdout 是一个 JSON 对象，只有一种形状：

  | 字段 | 类型 | 含义 |
  | --- | --- | --- |
  | `total` | 非负整数 | 符合条件的记录条数：过滤之后、按 `--limit` 截断之前 |
  | `records` | 数组 | 记录，新的在前（`created_at` 降序，相同时 `id` 降序）；带 `--limit N` 时最多 N 条，不带时全给 |

- 过滤和文字版一样：默认只给当前目录所在项目里没被删除、没被撤回、没被取代的记录（各种 `kind` 都在内）；`--all` 全给；`--line` 只给当前工作线的。
- 还没有数据库、项目从没采用过、没有符合条件的记录：都是 `{"total":0,"records":[]}`，退出码 0。
- 出错（比如数据库版本比程序新）：退出码 1，stderr 一行原因。
- `records` 里每条的字段：

  | 字段 | 类型 | 含义 |
  | --- | --- | --- |
  | `id` | 字符串 | 记录编号 |
  | `created_at` | 字符串 | 保存时间（运行 `save` 的时间，不是收取时间），RFC 3339 UTC 毫秒，如 `2026-10-10T08:50:43.687Z` |
  | `agent` | 字符串 | `claude`、`codex`，或 `local`（不属于哪一家的 hook 会话：`save` 没声明来源或声明的来源没出现过，以及用户命令写的更正、撤回、恢复）。以后可能有新值 |
  | `session_id` | 字符串或 `null` | 那家 agent 的会话编号；`agent` 是 `local` 时为 `null` |
  | `source_id` | 字符串 | 来源：`<agent>:<session_id>`，或 `local:<编号>` |
  | `line_path` | 字符串 | 工作线（worktree 根目录的绝对路径） |
  | `branch` | 字符串或 `null` | 保存时所在的分支；没有时为 `null` |
  | `kind` | 字符串 | `checkpoint`、`correction`、`retraction`、`restore`。以后可能有新值 |
  | `target_id` | 字符串或 `null` | 更正、撤回、恢复所指的记录编号；`checkpoint` 为 `null` |
  | `deleted_at` | 字符串或 `null` | 正文被删除的时间；没删除为 `null` |
  | `replaced_by` | 字符串或 `null` | 取代它的记录编号；没被取代为 `null` |
  | `retracted` | 布尔 | 是否被撤回 |
  | `summary` | 字符串 | 一行摘要，和文字版 `cairn list` 每行末尾那段一样（正文“## 停点”一节的第一个非空行）；没有时是空字符串 |

  状态由 `deleted_at`、`replaced_by`、`retracted` 三个字段表示：三个都是空（`null`、`null`、`false`）就是可见的记录，默认的列表只给这种。

### 2. `cairn show <ID> --json` 写成约定

现有行为不改，只把下面这些写成约定，并用测试钉住。现有输出里别的字段（`project_id`、`project_key`、`association`、`facts`、中文文字数组 `status`）保留，不进约定。

- 和 `cairn show --json` 一样先收取暂存区（会写数据库，旧版本的库也在这时升级）。`<ID>` 不限于当前目录所在的项目。
- 还没有数据库：退出码 0，stdout `{"status":"no_data"}`。
- 记录不存在：退出码 1，stderr 一行 `记录不存在`。
- 否则退出码 0，stdout 是一个 JSON 对象：上面 `records` 每条里除 `summary` 外的全部字段（同名同义），加上：

  | 字段 | 类型 | 含义 |
  | --- | --- | --- |
  | `body` | 字符串或 `null` | 正文原文（Markdown）；正文已删除，或这条记录本来没有正文（撤回、恢复）时为 `null` |
  | `correction` | 对象或 `null` | 这条记录最新的一条更正，没有或正文已删除时为 `null`。对象里是上面同样的字段（含 `body`，没有自己的 `correction`） |

### 3. 文档和版本号

- `docs/DESIGN.md`：§7 命令清单里 `list` 一行补上 `--json`、`--limit`；§7.1 加 `cairn list --json` 和 `cairn show <ID> --json` 两条，内容照上面第 1、2 节（写明各自读不读写数据库）；§7.1 开头那句“没列在这里的输出”里把 `show <ID> --json` 去掉，改成说明它只有列出的字段是约定。§7.1 已有的条目不改。
- `README.md`、`README.zh-CN.md`：`cairn list` 一行补上 `--json`。
- 版本号 0.2.0 → 0.3.0（`cairn --version` 能看出是哪一版）。

## 怎么算做完
- paddock 的需求原文（`paddock/docs/任务/P5-79-Cairn标签改成记录列表.md`「cairn 给的东西」），逐条达到：
  1. “`cairn list --json`：列出当前目录所在项目的记录，新的在前。每条至少有：记录编号、保存时间、哪家 agent（Claude、Codex，或者都不是）、来源、分支、种类、状态、一行摘要（和 `cairn list --line` 每行末尾那段一样）。
     - 能限制条数（面板每 5 秒读一次，记录会越积越多），并且能知道一共有多少条（面板要写总数和“还有 N 条更早的”）。
     - 默认只给可见的记录（和现在的 `cairn list` 一样）。
     - 最好和 `cairn status` 一样只读：不收取暂存区、不建库、不升级库。还没有数据库、项目没采用时给什么，请写明。”
  2. “`cairn show <ID> --json`（单条记录）写进公开约定，至少：编号、保存时间、哪家 agent、来源、分支、种类、状态、正文。它读不读写数据库请写明。”
  3. “两项都写进 cairn DESIGN §7.1（只加不改）”
- 输出和上面「要做的」里的字段表一致。
- 验证只做这些：先在 `crates/cairn/tests/cmds.rs` 写针对上面约定的测试，看到它们因为没实现而失败，再实现；`CARGO_TARGET_DIR=… cargo test --all-targets`、`cargo clippy --all-targets -- -D warnings`、`cargo fmt --check`、`git diff --check` 各跑一次。觉得不够，在回复里说，不要自己加。

## 不要做
- 不改表结构，不改数据库的写入路径，不改 hook。
- 不改不带 `--json` 的 `list`、`show` 的行为和文字输出；`show <ID> --json` 现有的字段不删、不改名。
- 不做：`show --json`（不带 ID）分节、翻页游标或偏移、按 agent 或分支筛选、给 `show <ID>` 改成只读。
- 不读用户真实的会话记录、真实的 cairn 数据库、各工具的记忆文件；不写 `~/.claude/settings.json`、`~/.codex/hooks.json`、`~/.codex/config.toml`。测试只用合成材料、临时 Git 仓库和隔离目录（临时 `HOME` / `XDG_STATE_HOME`）。
- 不运行 `cargo install`，不动 `~/.cargo/bin/cairn`。
- 不用 Python。
- 不要按项目名或路径批量杀进程（`pkill -f cairn` 这类）：主控和别的 agent 的进程命令行里都带着项目名和工作目录。
- 不碰 `corral ls` 里别的 agent。
- 现有代码和上面的字段表对不上（比如某个字段实际取不到、含义和表里写的不一样）：停下来报告，等决定，不要自己改约定。
- 不合并到 main，不推送。只在 `f3-list-json` 上提交。

## 做完
在本文件末尾追加「## 完成记录」（在你的分支里提交）：做了什么、验证了什么、拿主意的地方、没做的事，各几句话。回复里只写这几样，加上有没有要主控决定的事。命令都在前台跑完，全部做完后，回复最后一行写 DONE。

## 完成记录

- 做了什么：增加只读的 `list --json [--limit N]`，支持和 `--line`、`--all` 组合，返回过滤后的总数与按保存时间、ID 降序排列的记录。DESIGN §7、§7.1 已补列表和单条记录的字段、退出码、空结果与数据库读写约定，原有公开条目保留。两个 README 补 `--json`，同时把该行的默认范围写准为当前项目；crate 和锁文件版本升到 0.3.0。
- 验证了什么：先在 `cmds.rs` 添加 6 项约定测试，修正一处合成暂存文件夹具的两行格式后，5 项列表测试因不支持 `--json` 失败，单条 `show` 的既有约定测试通过。实现后 6 项全绿；覆盖字段及来源、保存时间、双键排序、限量和总数、各状态与 kind、项目和工作线过滤、空结果、版本过新错误、v1 只读不升级不收取，以及 `show` 的收取升级、跨项目读取、正文、更正和错误输出。`--version` 输出也固定为 `cairn 0.3.0`。随后 `cargo test --all-targets`（119 项通过）、`cargo clippy --all-targets -- -D warnings`、`cargo fmt --check`、`git diff --check` 各运行一次并通过；所有命令均在前台等待结束。
- 拿主意的地方：沿用 `Store::open_read_only`，在一个只读事务内读取列表，复用现有 `render::records` 的状态和排序；按现有列表规则过滤后统计总数，再截断并补来源字段。`--limit` 限制输出条数，底层仍沿用现有的项目记录读取方式。本轮没有另建查询模型或改存储层，单条 `show` 无需修改实现。没有要主控决定的事项。
- 没做的事：没有改表结构、写入路径、hook、文字输出或单条 `show` 的既有字段；没有读取真实会话、真实 cairn 数据库或修改真实配置，没有安装、合并、推送、操作其他 agent，也没有改任务范围以外的文件。测试只用隔离目录、合成数据和临时 Git 仓库。
