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

## 完成记录

2026-10-05，cairn/dev-save（Codex），分支 `p2c-save`。

**做了什么**：实现 `adopt` / `unadopt` / `save` 三个 CLI 子命令。save 只校验正文、调用 2b 采集事实并写暂存文件，不打开数据库；采用状态和取代对象在收取事务里核对。新增 `adopt`、`cli`、`save`、`spool`、`ingest` 模块；存储、表结构、作用域与事实采集的原文件均未改动。

暂存目录用目标数据库词法路径的 SHA-256 前 16 位隔离，逐层用目录句柄打开并核验属主/权限，临时文件独占创建、fsync 后通过 macOS `RENAME_EXCL` 发布。`ingest::ingest(&mut Store, &Spool, preferred_source)` 提供来源优先、50 个文件 / 300 ms 的收取预算；文件读取和 SQLite 锁等待也受剩余时间约束，收取结束恢复调用者的 busy timeout。每个操作独立 IMMEDIATE 事务，以 `spool_ops` 去重；提交后才删除文件，忙或 SQL 错误回滚当前操作并留文件，之前已经提交的操作不回退。`Spool::status()` 返回目录路径、待收取 JSON 和残留 tmp 数量，忽略链接和非普通条目。

**验证了什么**：所有命令均在前台等待完成，所有 Cargo 命令使用指定共享编译目录。先看到以下有效 RED，再实现对应行为并转为 GREEN：无效正文被原空 CLI 错误接受；合法 save / adopt 尚未实现；跨工作线取代错误入库；socket 条目使收取报错；worktree 的输出把工作线目录名误当项目短名。编译错误和最初 socket 路径过长的夹具错误未计作 RED。

`crates/cairn/tests/save.rs` 共 15 项测试，覆盖任务五条验收和四组补充要求：未采用拒收、正文校验、取代整条拒收、来源解析、并发 save；正文/无新内容/拒收的提交后重放、不同状态库隔离、目录与条目链接、不覆盖发布。直接相关边角用例包括：

- adopt / unadopt 重复执行保留状态变更时间；先收取后改变采用状态；worktree 共用项目，输出项目短名。
- 空正文、UTF-8 字节上限（正好 6 KiB 可接受）、`--nothing-new` 完全不读 stdin；目标数据库父路径不可用仍能 save。
- 跨项目或不存在的取代对象、重复声明同一合法对象；已知来源保持关联，缺失/未知来源用各自 `local:<op_id>`；确认时间保留暂存时间。
- 私有目录与文件权限；宽权限目录拒绝；目录、FIFO、socket 跳过；目标路径不匹配且正文损坏的文件不解析、不拒收、不删除。
- ID 不匹配、未知版本、非法时间和损坏 payload 拒收，拒收记录不含合成正文；既有 final 和 tmp 均不覆盖，残留 tmp 可统计。
- 优先来源先入库，51 个操作一次只处理 50 个；数据库锁冲突和确认写入失败均留文件且完整回滚，释放锁/撤销合成故障后可重试；Git HEAD、分支及持久化事实时间格式。

最终 `cargo test --all-targets` 一次通过（新增 save 15、原 scope/facts 5、原 store 17、probe 4，共 41 项）；`cargo clippy --all-targets -- -D warnings` 一次通过。另完成格式化和 `git diff --check`。数据全为合成材料，使用临时 HOME / XDG_STATE_HOME、临时 Git 仓库；成功路径经 `cli::run_at` 显式传入临时可信根，没有生产 CLI 环境开关，没有在真实可信根下建立 `cairn-spool`。

**拿主意的地方**：v1 文件是一个完整 JSON 对象，首行放 `header`（版本、操作 ID、数据库路径、声明来源），后面放 `payload`。收取先逐字节读头部，保证目标路径不匹配时不会预读正文；头部损坏、无法可靠辨明目标时保留文件。文件名不是规范 ULID 时跳过；文件名与内容 ID 不同则以文件名 ID 记幂等拒收，避免内容中的另一个 ID 占用其他操作。错误原因使用固定文字，不把解析错误中的正文写进事件。

`save::StoredFacts` 在持久化边界把 2b 的 Unix 毫秒转换为 `collected_at` 的 RFC 3339 UTC 毫秒字符串，其余事实字段保持原义；所有新增数据库时间列同样使用该格式。重复 adopt / unadopt 不刷新时间，未登记项目的 unadopt 为无操作；重新 adopt 更新采用时间并保留上次停用时间。`--nothing-new` 与 `--supersedes` 互斥，因为无新记录可承载取代关系；重复合法取代 ID 只写一次关系。库级调用者须用同一个 `database_path` 结果打开 Store 与 Spool，接口注释已写明。

**没做的事**：未做渲染、注入、回合判定、hook、其他命令或 install；未修改 DESIGN、HANDOFF、地基模块及真实配置，未读取真实会话或真实 cairn 数据库。未启动/委派 agent，未合并 main，未推送。收取时间预算是文件读取、文件步骤和锁等待之间的协作式截止，不声称能强制中断操作系统内正在执行的系统调用。没有需要主控决定的设计变更；交叉审查与合并留给主控。

## 返工记录

2026-10-05，按交叉审查第 1、2 条返工；主仓库的交叉审查文件仅只读查看，未修改。

**修改**：移除来源优先路径的全量头部预扫描和优先级排序。现在只先按文件名排列目录项，第一遍遇到匹配优先来源的头部就立即读取本目标 payload、执行单文件事务并删除已提交文件，不再等待剩余头部；第一遍完成后才在第二遍处理其他来源。两遍保留各自的 ULID 顺序，共用原来的 50 个文件 / 300 ms 预算和事务/删除顺序；没有加长超时，也没有跨调用的内存游标依赖。

新增 `ingest::pending(&Store, &Spool, source, since, budget) -> Pending`，供 2e 使用。数据库目标取 Spool 的词法路径，Store 仍须用同一个 `database_path` 结果打开；source 对应头部的声明来源，时间窗为 `created_at >= since`（包含起点，参数须为统一 UTC 毫秒格式）。排除 `spool_ops` 中已提交但未删除的操作后，只要找到一条确定匹配的文件便返回 `Yes`；完整扫描未发现匹配返回 `No`；读取/SQL 错误、无法辨认的头部或 payload、非法时间及预算耗尽返回 `Unknown`。调用者应对 Unknown 按 hook 错误放行策略处理，不能当作“没有”。

查询只观察文件和数据库，不写记录、不拒收、不删除文件；目录枚举、打开和读取继续相对已核验的目录句柄，目标路径不匹配时不读正文。内部候选结果区分正常跳过与未知；目录枚举超时明确报告未完成，不再把半份列表作为完整结果返回。文件读取和 SQL 锁等待使用查询剩余预算，结束后恢复连接原有 busy timeout。该接口不提供跨文件系统与数据库的原子快照，也不锁住并发发布/收取。

**验证**：只新增并运行两类测试，共 3 项。`priority_backlog_makes_progress_on_every_collection` 用 `Spool::publish` 写入 4,000 个规范 ULID、合法时间、同一已知来源的 `nothing_new` 文件：修改前确认第一轮 `processed=0` 的有效 RED；修改后连续三轮均处理 1–50 个，每轮重新打开 Spool，核对待收取数持续减少、确认数增加、处理 ID 按 ULID 前进。两项 `pending_query_*` 测试先确认缺少查询行为的 RED，再验证 Yes / No / Unknown，包括其他来源、窗口前/起点/起点后、不同数据库目标且 payload 损坏、已提交未删除、tmp 和链接、目录路径被替换但原句柄继续使用，以及损坏头部/payload、文件无读权限、SQL 读取错误、零预算和读取途中耗尽正预算；同时检查查询不写库、不删改文件及 busy timeout 恢复。一次夹具中生成时间早于固定窗口起点的问题已改为显式合成时间，不计作有效 RED。

最终 `cargo test --all-targets` 一次通过（save 18、scope/facts 5、store 17、probe 4，共 44 项），`cargo clippy --all-targets -- -D warnings` 一次通过；格式化与 `git diff --check` 通过。所有命令均前台等待结束，Cargo 使用指定共享编译目录；测试使用临时 HOME / XDG_STATE_HOME 和显式临时可信根，无真实 agent、真实配置或真实 cairn 数据访问。本次仅修改 `ingest.rs`、`spool.rs`、`tests/save.rs` 和本任务文件；未改 DESIGN、地基模块或表结构，未合并、未推送。

### 第二轮说明

2026-10-05。主控已通过 main 的 `9585ea1` 明确“必须向前推进”和“来源优先尽力而为”；先按指示在 `p2c-save` 执行 `git merge main`，合并提交为 `d3460f1`，只带入 DESIGN §6.5 / §8.3 的六行差异，无冲突。主仓库交叉审查文件仍只读，未修改。

**实现**：收取总预算保持 50 个文件 / 300 ms。目录枚举与优先来源发现共用调用开始后的前 100 ms，找到 50 个优先候选时可提前结束扫描。优先处理已找到的当前来源文件，再处理已找到的其他来源，最后沿尚未扫描的文件名继续收取；各组内保留 ULID 顺序，不再开启会耗尽全部预算的“只扫描优先来源”第二轮。扫描阶段只保留候选下标，文件句柄及时释放；处理时重新通过已有安全入口核验目标。若某个头部读到扫描截止点，保留其下标供剩余预算重试，不把它误当成已跳过的文件。没有优先来源时直接沿 ULID 收取。

这样在优先来源不存在、或位于大量其他来源积压之后时，已发现的其他可收取文件仍能使用预留时间提交，积压持续缩小后靠后的优先文件也会被收进来。单文件 IMMEDIATE 事务、幂等和提交后删除逻辑保持不变；没有扩大总预算、修改原有断言或取消“目标不匹配时不读正文”的检查。三态 pending 查询和地基模块未改。

**新增回归**：仅新增 `cross_source_backlog_always_progresses_and_eventually_collects_late_priority`。用 `Spool::publish` 发布 4,000 个来源 A 的合法 `nothing_new` 操作，指定已登记但没有文件的来源 B 连续调用三次；随后发布一条来源 B 的合法文件，其 ULID 排在所有 A 文件之后，继续调用直到 B 入库。每次重新打开 Spool，断言处理 1–50 个、确认数增加、待收取数相应减少，并核对 A 的确认保持 ULID 顺序、B 最终有确认且暂存文件已删除。循环上界为发布 B 时的剩余操作数，由“每次至少提交一笔”直接推出，没有放宽为允许零进展。

RED：原两遍实现于“B 无文件”的第一轮返回 `processed=0, ingested=0, rejected=0, replayed=0`，新测试失败。GREEN：改动后同一测试覆盖两种排列并通过（约 29 s，包含发布 4,000 个需 fsync 的文件）。除此之外只运行一次 `cargo test --all-targets`（save 19、scope/facts 5、store 17、probe 4，共 45 项全部通过）及一次 `cargo clippy --all-targets -- -D warnings`（通过）；格式化和差异空白检查通过。

所有命令均前台等待结束，Cargo 使用指定共享编译目录；测试仍只用合成材料、临时 HOME / XDG_STATE_HOME 和显式临时可信根。本轮实现提交只涉及 `ingest.rs`、`tests/save.rs` 和本任务文件，没有自行改设计、表结构或真实配置；未合并回 main、未推送。没有新增需主控决定的事项。

## 主控审查

2026-10-05，cairn/main。结论：通过，已合并。
- 初审：没动地基文件和 DESIGN；验收 5 条与 §6.5 的四组补充要求都有测试；真实临时目录下没有留下 `cairn-spool`。
- 交叉审查（`docs/tasks/P2c-adopt与save-交叉审查.md`）提出 1 条必须改：来源优先收取在积压下反复零进展。两轮返工后关闭；期间主控改了 DESIGN §6.5（9585ea1）：收取必须向前推进，来源优先改为尽力而为。建议改（给 2e 的"未处理暂存文件"有预算查询）已一并实现。
- 第 1 条验收按 DESIGN 修订落在收取时（save 不读数据库），已告知用户。
- 合并后 main 上 `cargo test --all-targets`（save 19、作用域 5、存储 17、探针 4）与 clippy 通过。save 测试因积压回归约需 50 秒。
