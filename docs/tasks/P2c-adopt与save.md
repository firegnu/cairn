# 任务：2c adopt / unadopt、save 与暂存区收取

2026-10-05，cairn/main（主控）交给 cairn/dev-save（Codex，重：gpt-6-astra / xhigh）。
路由：重 / 交叉审查要 / 影响面：碰要害（路由：重、要、碰要害）
类型：功能变更
依据：本轮做 `cairn adopt` / `unadopt`、`cairn save`（只写暂存区）和收取函数（暂存文件进数据库）；不做注入渲染（2d）、回合判定（2e）、其他用户命令（2f）、hook 适配（阶段 3）。
提示：围绕已确认的使用目标完成变更，优先沿用现有机制。
你是被委派的 agent：照本文件做，不要再开别的 agent。

## 先读
- `AGENTS.md`（规矩一节）
- `docs/DESIGN.md` §3、§5、§6.1、§6.2、**§6.5（全文，这是本任务的核心）**、§7（save 一行）、§8.2、§12
- `docs/实施计划.md` 阶段 2 的 2c 一行
- 已合并的地基（直接用，不要改它们的行为）：`crates/cairn/src/store.rs`（`database_path`、`Store::open`、`transaction`）、`scope.rs`（`Git::resolve`）、`facts.rs`（`Git::collect`）
- `docs/tasks/P2a-存储-交叉审查.md` 的审查意见（了解存储层对符号链接、权限、并发首次打开的约定）

## 在哪里干活
- worktree：`/Users/firegnu/Developer/personal_projs/cairn-worktrees/p2c-save`，分支 `p2c-save`（已从 main 建好）。
- 可以新建模块文件（例如 `spool.rs`、`ingest.rs`、`adopt.rs`），在 `lib.rs` 里声明；`main.rs` 写成命令行入口，本轮只接 `adopt`、`unadopt`、`save` 三个子命令，结构留给后面的任务加子命令。测试放 `crates/cairn/tests/` 下。可以往 `crates/cairn/Cargo.toml` 加依赖。
- 不要改 `store.rs`、`store/schema.rs`、`scope.rs`、`facts.rs` 的已有行为。确实需要它们新增一个小接口，停下来报告。
- 编译：所有 cargo 命令前加 `CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target`。

## 要做的
- **时间格式**：所有写进数据库和暂存文件的时间，统一用 RFC 3339 UTC、毫秒精度、`Z` 结尾（例如 `2026-10-05T12:00:00.000Z`），保证字符串比较就是时间比较。2d、2e 会依赖这一点。
- **`cairn adopt` / `unadopt`**：作用于当前目录所在的项目（项目键按 2b 的 `resolve`）。adopt 写或更新 `projects`（`adopted=1`、`adopted_at`），unadopt 置 `adopted=0`、`unadopted_at`；重复执行结果不变。按 §6.5，用户命令执行前先收取暂存区。
- **`cairn save [--source ID] [--nothing-new] [--supersedes ID]...`**：照 §8.2 "save 进程里"的 1–4 步和 §6.5 "写入"。
  - 正文从 stdin 读；校验：非空、必须有 `## 停点` 一节、不超过 6 KiB。`--nothing-new` 时不读正文。校验不通过返回非 0，stderr 一行原因，不写暂存文件。
  - 采集事实用 2b 的 `collect`，git 已经带 `--no-optional-locks`。
  - **不打开数据库。** 项目是否采用、取代对象是否合规，都留到收取时核对（这是 DESIGN 修订后的行为，见 §8.2 最后一段）。
  - 成功时 stdout 一行，格式参考 §7（含操作 ID、项目短名、分支、短 HEAD）。
- **暂存区**：照 §6.5 "位置与隔离""文件安全""写入"全文实现：
  - 可信根取 `confstr(_CS_DARWIN_USER_TEMP_DIR)`，取不到退回 `$TMPDIR`；可信根条件、逐层 `openat(O_DIRECTORY | O_NOFOLLOW)`、打开后 `fstat` 核验；
  - 命名空间 `ns` = 目标数据库路径（2a 的 `database_path`）的 SHA-256 前 16 位十六进制，文件里也写上目标数据库路径；
  - `.<op_id>.tmp` 用 `O_CREAT | O_EXCL | O_NOFOLLOW`、0600 创建，`fsync` 后 `renameatx_np(..., RENAME_EXCL)` 发布成 `<op_id>.json`；系统不支持 `RENAME_EXCL` 就报错，不退回普通改名；
  - 暂存格式带版本号。
- **收取函数**（给 2d、2e、2f 和阶段 3 的 hook 调用，本轮由 adopt / unadopt 调用）：照 §6.5 "收取"全文：
  - 只处理本 `ns` 的 `.json`，目标路径对不上就跳过，不读正文、不拒收、不删除；
  - 可以指定"优先处理的来源"；单次上限 50 个文件或 300 ms；
  - 每个文件一个 `BEGIN IMMEDIATE` 事务：查 `spool_ops` → 按 §5 / §8.2 第 5 步解析来源（`local:<op_id>`，关联不确定）→ 校验采用和取代对象（§8.2 第 6–7 步）→ 写记录（有正文时）、取代关系、`confirmations`（带 `op_id`，时间用暂存文件的 `created_at`）或 `save_rejected` 事件（不含正文）→ 写 `spool_ops` → 提交后删除文件；
  - 文件已被别人删掉算正常竞争；数据库忙则回滚、留文件；
  - 条目不是普通文件、属主不对、文件名 ID 和内容 ID 不一致：按 §6.5 跳过或拒收。
  - 再提供一个小函数，报告本 `ns` 下待收取的 `.json` 和残留 `.tmp` 的数量与目录路径（给以后的 `cairn status` 用）。
- 依赖只用成熟、活跃维护的库（例如 `ulid`、`sha2`、`clap`；`libc` 已在依赖里）。

## 怎么算做完
以下验收条件经用户 2026-10-05 同意，取自实施计划 2c 的"建议验证"。其中第 1 条按 DESIGN 修订（save 不读数据库）落在收取时：
1. 项目未采用时：save 的暂存文件在收取时被拒收（记 `save_rejected`，不写记录、不写确认，文件删除）。
2. 正文超长或缺"停点"时：save 直接拒绝，返回非 0，不写暂存文件。
3. 取代对象跨工作线、未注入过或已删除时：整条拒收，不写入任何记录、取代关系或确认。
4. 没带来源（或来源从未出现过）时：分配 `local:<op_id>`，标为"关联不确定"。
5. 并发 save 互不覆盖：多个 save 同时写同一个暂存区，每个都能各自收进来。

另外按 DESIGN §6.5，以下各有一个测试：
- 同一个暂存文件被收取两次（模拟"提交后、删文件前进程退出"），只写一次，结果不变；正文、`--nothing-new`、拒收三条路径都要覆盖。
- 两个不同状态库（不同 `XDG_STATE_HOME`）的暂存文件互不处理。
- 暂存区目录或条目是符号链接时拒绝 / 跳过，不读、不删、不改链接目标。
- 发布时目标名已存在不会被覆盖。

验证只做这些：上面各条一个测试，再补你判断直接相关的边角用例（在完成记录里列出）；`cargo test --all-targets` 和 `cargo clippy --all-targets -- -D warnings` 各跑一次并通过。测试一律用临时目录：临时 `XDG_STATE_HOME`、临时 `HOME`、临时 Git 仓库；暂存区的可信根在测试里要能指到临时目录（例如通过 `TMPDIR` 或测试专用参数），**不要在真实的 `DARWIN_USER_TEMP_DIR` 下留下文件**。觉得不够，在回复里说，不要自己加。

## 不要做
- 不要读写用户真实的 cairn 数据库，不要在真实的 `~/.local/state`、真实临时目录下创建 `cairn-spool`。
- 不要读真实会话记录、各工具的记忆文件。实现、测试、辅助脚本都不要用 Python。
- 不要做渲染、注入、回合判定、其他用户命令、hook 入口、install。
- 不要改 `store.rs`、`schema.rs`、`scope.rs`、`facts.rs` 的已有行为，不要改表结构。DESIGN 说不通的地方停下来报告，等主控决定。
- 不要按项目名或路径批量杀进程（`pkill -f cairn` 这类）。
- 不合并到 main，不推送。只在 `p2c-save` 分支上提交。

## 做完
在本文件末尾追加「## 完成记录」（在你的分支里提交）：做了什么、验证了什么、拿主意的地方、没做的事，各几句话。回复里只写这几样，加上有没有要主控决定的事。命令都在前台跑完，全部做完后，回复最后一行写 DONE。
