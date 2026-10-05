# 交叉审查：2a 存储层

2026-10-05，cairn/main（主控）交给 cairn/dev-review-store（Codex，重：gpt-6-astra / xhigh）。
类型：调研
提示：区分已查证事实、推测和未知，给出直接依据；结论限定在本轮调查范围。
你是被委派的审查者：只读审查，不要开别的 agent。**本文件在主仓库工作区（`/Users/firegnu/Developer/personal_projs/cairn/docs/tasks/`），不在你的审查 worktree 里；你只写本文件这一个文件。**

## 背景
- cairn/dev-store（Codex）在分支 `p2a-store` 上实现了 SQLite 存储层（`crates/cairn/src/store.rs`、`store/schema.rs`、`tests/store.rs`），提交 `ae1329e`。任务书：`docs/tasks/P2a-存储.md`（在你的审查 worktree 里也有，末尾有对方的完成记录）。
- 主控已审过：只改了允许的文件；验收 3 条（迁移幂等、权限 0700/0600、删除后数据库和 WAL 里找不到原文）和"只读打开不建库、新版本库报错不改库"都有测试；重跑 `cargo test --all-targets`（13 + 4 项通过）、clippy、`git diff --check` 均通过。
- 这是后续 2c–2f（save 与收取、渲染、回合判定、用户命令）的地基。

## 先读
- `AGENTS.md`（规矩一节）
- `docs/tasks/P2a-存储.md`（任务要求与完成记录）
- `docs/DESIGN.md` §3 术语、§6.1、§6.2、§6.5（和表有关的部分）、§8.2、§8.3、§8.5、§12

## 要审查的
- 审查 worktree：`/Users/firegnu/Developer/personal_projs/cairn-worktrees/review-p2a-store`（detached，指向 `ae1329e`）。改动范围：`git diff 12e9feb...HEAD`。**只读：不改、不提交、不切分支。**
- 可以跑：`CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target cargo test -p cairn --test store`、clippy；需要验证某个具体怀疑时，可以在临时目录里写一次性的 Rust 测试或用 `sqlite3` 命令行核对，但不要把它们留在 worktree 里。
- 不要运行 claude、codex，不要开 agent；不要读真实的 `~/.local/state`、真实会话记录、工具记忆、Corral 事件文件。不要用 Python。
- 验证预算：只针对下面的重点做定点核实；不录屏，不做覆盖矩阵。

## 重点看
1. **正文删除是否真的不留原文**（§12）：`secure_delete` 在 WAL 模式下的实际效果；`wal_checkpoint(TRUNCATE)` 返回值的判定；检查点忙时的错误语义和重试是否安全；有没有别的地方会留下原文（例如 freelist、`-shm`、被覆盖前的旧页）。
2. **权限与文件创建**：目录 0700、库和 `-wal` / `-shm` 不宽于 0600 的保证是否在各种时序下成立（SQLite 之后新建 sidecar、umask、并发首次创建）；创建和 chmod 时是否会跟随符号链接，影响有多大。
3. **迁移与版本**：两个进程同时首次打开同一个新库；迁移中途失败；库版本比程序新时是否真的一个字节都不改。
4. **表结构是否撑得住后续流程**：对照 DESIGN §6.2、§6.5、§8.2、§8.3，约束、外键、唯一键、索引会不会挡住 2c–2f 要做的事（收取事务的幂等、`local:` 来源、续跑去重、更正 / 撤回 / 恢复、删除墓碑），或者漏了它们需要的约束。
5. **只读打开**：库不存在时不创建任何东西；库存在时只读连接上设 pragma 的行为；权限过宽时拒绝是否合理。

## 输出
追加到本文件末尾「## 审查意见」：
- 先写一句结论：**可以合并** / **改完再合并**。
- 每条意见写：级别（必须改 / 建议改 / 可以不改）、位置（文件和行号）、问题、改法。
- **"必须改"只给两种情况**：任务书"怎么算做完"没达到；或者碰到的要害上有真实缺陷（会丢数据、并发出错、隐私正文残留、越权、核心规则算错）。其余一律"建议改"。能给出具体复现步骤或直接依据的写上。
- 最后对完成记录里"拿主意的地方"逐条表态：同意 / 不同意及理由。

## 回复
回复里只写结论和各级别条数。命令都在前台跑完，全部做完后，回复最后一行写 DONE。

## 审查意见

**改完再合并**。必须改 2 条，建议改 1 条，可以不改 4 条。

2026-10-05，cairn/dev-review-store；审查对象为 detached HEAD `ae1329ef66dbdb3cd8b9c787f513eeafdd175b60`，范围 `12e9feb...HEAD`。以下区分实测结果和结论边界，没有修改实现或仓库测试。

1. **必须改：符号链接使权限检查与 SQLite 实际使用的文件不一致。**

   位置：`crates/cairn/src/store.rs:97–124`、`:142–156`、`:219–223`。

   问题：主库可以是符号链接；程序用传入路径的父目录和拼接出的 `-wal` / `-shm` 做 chmod 或权限检查，但 SQLite 会使用实际主库旁的 sidecar。因而两个打开接口都可能成功，同时实际库目录和含正文的 WAL 仍为宽权限。这直接不满足权限验收。

   已查证的复现：在临时目录内用 `Store::open(real/cairn.db)` 建库并保持 writer 打开，写入合成正文；把 `real/` 设为 0755、实际 WAL/SHM 设为 0666，主库保持 0600。在另一个 0700 的 `alias/` 中建立 `cairn.db -> real/cairn.db`，依次用链接路径调用 `open_read_only` 和 `open`。两者均成功，实际目录仍为 0755，实际 WAL/SHM 仍为 0666，WAL 原始字节中存在合成正文。无需制造检查与使用之间的竞态。

   同一类路径处理还会修改无关目标的权限：新库旁预放 `cairn.db-wal` 或 `cairn.db-shm` 符号链接，指向一个合成的 0644 普通文件，调用 `open` 后，目标权限都变为 0600；SHM 情形最终打开报错，但 chmod 已发生。此次没有观测到这两个无关目标的内容被覆盖，不把它夸大为已证实的数据破坏。

   改法：先明确并实施库目录、主库、sidecar 的链接策略。最小可行方向是在 SQLite 探测与 chmod 之前拒绝受控位置上的符号链接，用不跟随链接的打开及句柄核验避免检查/使用错位；若保留链接支持，权限验证必须覆盖 SQLite 实际使用的父目录和 sidecar，并防止路径被替换。不要继续对未经核验的 sidecar 路径直接 chmod。补上上述主库链接和 sidecar 链接的定点回归。

2. **必须改：版本预检会在权限收紧之前新建宽权限 sidecar，错误返回后仍然留下。**

   位置：`crates/cairn/src/store.rs:90–96`；对照 `:105–123`。

   问题：`SQLITE_OPEN_READ_ONLY` 不代表无文件系统写入；读取 WAL 模式库时仍可能创建 WAL/SHM。预检先于全部权限处理，且发现未来版本会立即返回，所以“之后再 chmod”覆盖不到这条错误路径。依据是任务明确要求 SQLite 生成的 WAL/SHM 不宽于 0600，不是把普通 SQLite 只读行为本身判成缺陷。

   已查证的复现：在临时库内把 `meta.schema_version` 更新为 `2`，关闭最后一个连接，确认两个 sidecar 都不存在；把主库设为 0644、父目录设为 0755，再调用 `Store::open`。返回 `NewerSchema { found: 2 }`，主库字节逐字未变，但新出现的 `cairn.db-wal` 和 `cairn.db-shm` 都为 0644。此例 WAL 是新建的空文件，没有观测到正文泄漏；仍然直接违反文件权限要求。对于受支持的宽权限 WAL 库，同样的预检还先于权限修复执行。

   改法：将安全判定放到任何 SQLite 打开/查询之前；一种保守实现是遇到不能安全探测的宽权限现有路径就无副作用拒绝，而不自动修复。若保留自动收紧策略，需要保证预检创建的文件自创建起就私有，并兼顾未来版本“不改库”的边界。不能简单提前 chmod 未来库，也不能用忽略 WAL 的 `immutable` 探测来替代当前的 WAL 版本检查。补“已关闭、无 sidecar、宽权限的未来 WAL 库”回归，并检查目录项和权限，不能只比较主库字节。

3. **建议改：明确并发首次打开可立即返回忙错误，并补该边界的验证。**

   位置：`crates/cairn/src/store.rs:88–130`、`:179–192`；`crates/cairn/tests/store.rs:69` 起的迁移测试。

   已查证：一次定点实验中，每轮两个普通 Rust 子进程通过 stdin 同步开始，对同一个新路径调用 `open(..., UserCommand)`；共 8 轮，7 轮有一方在 0–1 ms 返回 `DatabaseBusy`，另一方成功，1 轮双方成功。每轮结束后重新打开并做 `PRAGMA integrity_check` 均为 `ok`。没有观测到半套表或版本损坏。该实验只证明并发初始化存在忙错误，不把具体失败点未经跟踪归因于某一条 SQL，也不据此估算实际发生率。

   分级理由：DESIGN §6.5 / §8.6 明确允许数据库忙时留待下次处理、hook 放行；`busy_timeout=2000` 也不是保证等待满两秒。SQLite 官方说明忙处理器可能为避免死锁而不被调用。因此这不是已证实的迁移一致性缺陷，不列为必须改。[SQLite busy handler 文档](https://sqlite.org/c3ref/busy_handler.html)

   改法：补一个并发首次打开的回归，验证成功或可重试忙错误后的完整性，并在接口约定中写明调用方须处理初始化阶段的忙错误。若后续确实需要提高首次打开成功率，只在原预算内重试可重试错误；不把加长超时作为修复。

4. **可以不改：正文删除、检查点忙的判定和重试语义成立。**

   位置：`crates/cairn/src/store.rs:195–215`；`crates/cairn/tests/store.rs:213–278`、`:420–468`。

   核实的问题：`secure_delete=ON` 的更新会清除普通表旧内容；成功的 TRUNCATE 检查点会截断 WAL，第一返回列为 1 表示未完成。因此读取第一列而非只看 SQL 是否执行成功是正确做法。[SQLite secure_delete 与 wal_checkpoint 文档](https://sqlite.org/pragma.html#pragma_secure_delete)、[检查点返回值](https://sqlite.org/pragma.html#pragma_wal_checkpoint)

   证据：现有删除/忙检查点测试重跑通过。另用约 5.9 KiB 的重复唯一合成标记，先 FULL checkpoint，确认主库与 WAL 含标记；删除成功后主库、WAL、SHM 均无标记，`freelist_count=1`、WAL 长度为 0。这覆盖了本例释放的 overflow/freelist 页。忙检查点时墓碑已经提交、旧 WAL 仍有正文；释放读事务后重试会继续清理，首次删除时间不变。

   改法：存储原语保持现状；2f 必须把 `CheckpointBusy` 当作物理清除尚未完成，不能因为 `body=NULL` 就报告成功。本结论限于当前十张普通表、Store 连接设置及可见数据库文件；不承诺抹除文件系统快照、备份或已被其他进程读入的内存，也未进行掉电实验。

5. **可以不改：迁移事务与已覆盖的未来版本数据保护成立。**

   位置：`crates/cairn/src/store.rs:179–192`、`:226–248`；`crates/cairn/tests/store.rs:153–180`、`:364–418`。

   核实的问题：迁移在 IMMEDIATE 事务内再次查版本，表创建与版本写入一起提交；中途冲突回滚。现有幂等、迁移失败、未来 rollback 库、版本仅在 WAL 中更新的测试均通过；未来版本的主库/现有 WAL 字节不变。并发首次打开后的完整性结果见第 3 条。

   改法：迁移事务保留。不能把这些证据扩展成“任何文件都不创建、不变”，预检的权限问题仍须按第 2 条处理；本轮未验证新旧程序同时升级同一库的跨版本竞争。

6. **可以不改：表结构足以承载本轮设计中的后续流程，未发现阻挡 2c–2f 的约束。**

   位置：`crates/cairn/src/store/schema.rs:9–68`。

   核实的问题：`sources.agent` 未锁死为两种 agent，`session_id` 可空，`association='uncertain'` 可表达 `local:<op_id>`。`spool_ops.op_id` 主键、`confirmations.op_id UNIQUE` 配合 IMMEDIATE 收取事务可实现重放幂等；`turn_decisions` 按来源/回合/结果唯一，允许同回合先请求续跑再确认，同时让重复 `continue_requested` 冲突。四种记录 kind、`target_id` 自外键、可空 body 与 deleted_at 支持追加更正/撤回/恢复及墓碑；删除不删记录行，不会破坏取代外键。

   改法：保持现状。原草案未设外键的事件、注入、确认、操作日志表不必为本任务强加外键；来源解析、跨工作线取代校验和只追加规则仍由后续事务实现，不能宣称当前 schema 已独立保证这些业务规则。现有五个索引方向合理，本轮没有查询规模或性能证据要求增删索引。

7. **可以不改：缺库只读不创建、正常只读连接的 pragma 与拒绝宽权限策略合理。**

   位置：`crates/cairn/src/store.rs:134–164`；`crates/cairn/tests/store.rs:183–191`、`:281–360`。

   核实的问题：缺库直接返回 None；现有 v1 WAL 库上设为已有的 WAL 模式不要求切换日志模式，foreign_keys/secure_delete 设置也不让连接取得写库能力。测试证明可读已提交 WAL、写 SQL 返回 ReadOnly、主库字节不变，并拒绝已检查到的宽权限路径。只读 WAL 连接为现有库创建私有 sidecar 是 SQLite 的正常行为，符合“不创建主库或目录”的约定。[SQLite 只读 WAL 文档](https://sqlite.org/wal.html#read_only_databases)

   改法：保留该行为和拒绝策略，但第 1 条所述实际路径漏检必须补上。不能把“不 chmod”解释为“SQLite 绝不写 sidecar”。

验证记录：前台完成 `CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target cargo test -p cairn --test store`（13/13）、`cargo clippy --all-targets -- -D warnings`（同一 CARGO_TARGET_DIR）和 `git diff --check 12e9feb...HEAD`，均通过。额外验证仅上述四组定点实验，临时 Rust 程序在 `/private/tmp/cairn-p2a-review.QSdxqa/`，依赖被审查 worktree 的 crate，使用同一个 bundled SQLite；`symlinks`、`future`、`erase`、`concurrent` 四个参数可分别复跑。数据均由 tempdir 创建并随实验回收，无仓库内探针、无真实 agent 实测、无配置修改。

对完成记录“拿主意的地方”逐条表态（以下是决策复核，不另计意见条数）：

- **同意**纯路径函数显式接收环境值、空 XDG 回退、拒绝相对根路径、词法规范化。它满足暂存命名空间的设计；该函数只算路径，不能替代实际打开时的安全核验。
- **同意**时间戳由调用方提供，以及 Hook 200 ms / UserCommand 2 s 两档设置；调用方仍需处理即时 Busy。
- **同意**保留十张表、文本主键 NOT NULL、adopted 0/1 和设计中列出的枚举约束；没有改变字段含义。
- **同意**保留原外键和唯一键，尤其是 confirmations.op_id 与 spool_ops.op_id；不要求本轮增加业务触发器。
- **同意**新增的五个索引及 events.kind 保持可扩展；后续业务语义仍须在事务里执行。
- **不同意当前“写入打开收紧宽权限”的实现顺序和覆盖范围**：存在第 1、2 条已复现漏洞。自动收紧这个目标可以保留，但不能以成功返回时的表面路径权限代替全程安全；安全拒绝也是可选实现。
- **同意**只读打开拒绝宽权限而不 chmod；前提是检查的是 SQLite 实际使用的目录和文件，第 1 条需修正。
- **同意**现有 WAL 库的只读连接可创建私有 sidecar；不创建主库/目录且不迁移的边界合理。
- **不同意把当前只读版本预检当作完整保护**：读取 WAL 中的版本是必要的，保护主库/现有 WAL 字节的方向同意，但它还会产生第 2 条副作用，必须一起处理。
- **同意**迁移使用 IMMEDIATE 并在锁内复查版本；一致性不能仅靠锁外探测。
- **同意**先提交墓碑再检查 TRUNCATE 返回状态、繁忙报错且允许重试；2f 需明确“墓碑已提交”和“物理清除完成”是不同结果。
- **同意**不存在 ID 返回 false、重复删除保留首次 deleted_at。这里是保留第一次写入的值，不是比较调用方传入时间戳后取数值上的最早时间。

## 复核意见

**可以合并**。必须改 0 条，建议改 0 条，可以不改 4 条；上一轮两条必须改均已修好。

2026-10-05，cairn/dev-review-store。当前 detached HEAD 为 `89453f5bf392f283217f1200bc75494d9572371a`。本次仅复核上一轮第 1、2 条和 `ae1329e..89453f5` 的新增影响，没有重新审查未改动的业务设计，也没有把无关旧问题升级为合并门槛。

1. **可以不改：上一轮第 1 条关闭，原有链接路径漏洞已修复。**

   位置：`crates/cairn/src/store.rs:94–110`、`:125–132`、`:197–269`；`crates/cairn/tests/store.rs:515–611`。

   已查证：两个打开接口均先进入同一个路径检查函数；库目录、主库和已有 sidecar 用 `symlink_metadata` 核验类型、属主和权限，受控位置的符号链接直接拒绝，主库尚不存在时也检查 sidecar。新主库使用 `create_new` / `O_NOFOLLOW`，SQLite 打开带 `SQLITE_OPEN_NOFOLLOW`；已移除会跟随 sidecar 链接的 chmod。

   复跑上一轮主库链接探针，两种打开方式都从接受变为拒绝。正式回归进一步验证库目录链接、WAL/SHM 链接和悬空链接：拒绝后目标内容、权限及链接保持不变，主库不存在时不会先建库。原来“检查别名旁的 sidecar，却打开真实主库旁宽权限 sidecar”的路径已被封住。改法：保留本次修复，不要求继续修改。

2. **可以不改：上一轮第 2 条关闭，宽权限未来库不再触发 SQLite 预检副作用。**

   位置：`crates/cairn/src/store.rs:94–106`、`:209–248`；`crates/cairn/tests/store.rs:615–648`。

   已查证：安全检查位于任何 SQLite 打开之前，宽权限输入在进入版本查询前返回 `InsecurePermissions`。上一轮“关闭未来 WAL 库、移除 sidecar、主库 0644 / 目录 0755”的原始探针现已得到：主库字节不变，WAL/SHM 均不存在；新增正式回归同时检查权限与目录项不变。

   没有以忽略 WAL 换取通过：原有“未来版本只在 WAL 中”测试仍通过。本轮临时补测还确认，0700/0600 的已关闭未来 WAL 库，经祖先目录别名打开时，两种接口均返回 `NewerSchema { found: 2 }`，主库字节不变，SQLite 若新建 sidecar，其权限不宽于 0600。改法：保留本次修复；不把“宽权限库先报权限错误”误判为版本检测回归。

3. **可以不改：同意“已有宽权限路径一律拒绝、不 chmod”的取舍。**

   位置：`crates/cairn/src/store.rs:88–91`、`:197–216`；`crates/cairn/tests/store.rs:326–342`；`docs/tasks/P2a-存储.md` 的返工记录。

   理由：自动收紧不是原任务验收要求，而是上一版实现自行选择的策略。拒绝能够同时满足“不经不安全路径打开 SQLite”和“不修改未来版本库的权限/内容”，也避免替链接目标改权限。新建库仍以 0700 目录和 0600 文件创建，正常私有库继续可读写，未牺牲正常路径的权限要求。

   代价是已有宽权限库不再自动修复，用户需先自行修正权限；权限不安全时也不会先查出并报告库版本。这是可以接受的保守错误语义，接口文档、错误信息和返工记录已明确。改法：保留该策略，不要求恢复 chmod。原未来版本/迁移失败夹具改为私有权限，是为了通过新的安全前置条件后继续检验原有断言；另有独立宽权限回归补齐拒绝行为，并非放宽版本保护或回滚断言。

4. **可以不改：在本次差异范围内未发现新问题。**

   位置：`crates/cairn/src/store.rs:92–93`、`:219–269`；`crates/cairn/tests/store.rs:652–704`；`crates/cairn/Cargo.toml:8`、`Cargo.lock`。

   已查证：主库创建竞争用独占创建、遇到 AlreadyExists 后重新检查来处理，不覆盖竞争方文件。新增并发初始化测试至少要求一方成功，其余只允许 DatabaseBusy，随后验证完整性、版本及十张表；没有增加内部重试、修改时间预算或削弱数据库完整性断言。新增接口说明与上一轮第 3 条建议一致。

   路径检查只规范化祖先别名，保留对受控库目录和文件链接的拒绝。本轮临时用祖先符号链接模拟别名，验证从别名路径新建库、写入和只读读取均成功，正常的祖先别名没有被 `SQLITE_OPEN_NOFOLLOW` 误拒绝。libc 仅新增为直接依赖，锁文件中未升级依赖版本；表结构、迁移事务及删除原语没有改变。改法：本轮无新增修改要求。

验证记录：所有命令均在前台等待结束，Cargo 均使用 `CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target`。`cargo test -p cairn --test store` 17/17 通过，`cargo clippy --all-targets -- -D warnings` 和 `git diff --check ae1329e..89453f5` 通过。另复跑原临时探针的 `symlinks`、`future`，并运行 `/private/tmp/cairn-p2a-review.QSdxqa/tests/recheck.rs` 的一项合法路径补测，通过。材料全部为临时目录内的合成数据；未修改实现、正式测试、配置、分支或提交，仅在本文件追加复核意见。结论限定于这两条原始问题及此次差异，没有据此宣称所有可能的文件系统并发攻击场景都已穷尽。
