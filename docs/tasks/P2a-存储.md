# 任务：2a 存储层（crates/cairn/src/store.rs）

2026-10-05，cairn/main（主控）交给 cairn/dev-store（Codex，重：gpt-6-astra / xhigh）。
路由：重 / 交叉审查要 / 影响面：碰要害（路由：重、要、碰要害）
类型：功能变更
依据：本轮只做 SQLite 存储层的地基（位置、权限、设置、表结构 v1、迁移、只读打开、正文删除）；不做 save、收取、渲染、回合判定等业务流程，那些在 2c–2f。
提示：围绕已确认的使用目标完成变更，优先沿用现有机制。
你是被委派的 agent：照本文件做，不要再开别的 agent。

## 先读
- `AGENTS.md`（规矩一节）
- `docs/DESIGN.md` §3 术语、§6.1、§6.2、§6.4、§6.5（只看和表有关的部分：`spool_ops`、`confirmations.op_id`）、§8.6、§12
- `docs/实施计划.md` 阶段 2 的 2a 一行

## 在哪里干活
- worktree：`/Users/firegnu/Developer/personal_projs/cairn-worktrees/p2a-store`，分支 `p2a-store`（已从 main 建好）。
- crate 骨架已在 main 建好：`crates/cairn`，`lib.rs` 已声明 `store`、`scope`、`facts` 三个模块。
- 只动：`crates/cairn/src/store.rs`（可以拆成 `store/` 目录下的子模块）、`crates/cairn/tests/` 下以 `store` 开头的测试文件、`crates/cairn/Cargo.toml` 的 `[dependencies]` / `[dev-dependencies]`、`Cargo.lock`，以及本任务文件末尾的完成记录。
- 并行任务：cairn/dev-scope 在分支 `p2b-scope` 上写 `scope.rs`、`facts.rs`。你不要动这两个文件和 `lib.rs`。两边都可能往 `Cargo.toml` 加依赖，合并冲突由主控处理。
- 编译：所有 cargo 命令前加 `CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target`。

## 要做的
- **位置与权限**（§6.1）：数据库在 `${XDG_STATE_HOME:-$HOME/.local/state}/cairn/cairn.db`。目录 0700，数据库文件 0600；SQLite 生成的 `-wal`、`-shm` 也不能比 0600 宽。路径的计算要能被别的模块复用：2c 的暂存区要按"目标数据库路径"算命名空间（§6.5），所以对外提供一个只算路径、不碰文件系统的函数。
- **打开方式**：
  - 写入打开：需要时创建目录和库，跑迁移。
  - 只读打开：库不存在时返回"尚无数据"（例如 `Ok(None)`），**不创建目录也不创建库文件**。
  - 打开时设置 `journal_mode=WAL`、`foreign_keys=ON`、`secure_delete=ON`。
  - `busy_timeout` 两档：hook 用（约 200 ms）、用户命令用（约 2 s），由调用方选。
- **表结构 v1**：按 §6.2 草案定稿（含 `spool_ops`、`confirmations.op_id UNIQUE`、`turn_decisions.outcome` 的取值）。草案里的字段可以补约束和索引；要改字段含义或删表，先停下来报告。表结构在代码里有版本号（`meta.schema_version`）。
- **迁移**：从空库迁到 v1；已经是 v1 的库再跑一次结果不变；库的版本比程序认识的更新时报错，不动库。
- **正文删除原语**（§3"删除"、§8.5 delete、§12）：给定记录 ID，把 `body` 置 NULL、写 `deleted_at`，然后 `PRAGMA wal_checkpoint(TRUNCATE)`。只做这个原语；`cairn delete` 命令本身在 2f。
- 对外接口只需要够 2c–2f 在上面写业务：连接的包装（含事务）、上面这些函数。不要提前写 save、收取、渲染的业务函数。

## 怎么算做完
以下验收条件经用户 2026-10-05 同意，取自实施计划 2a 的"建议验证"：
- 迁移幂等：同一个库连续迁移两次，表结构和数据都不变。
- 权限：目录 0700、库文件 0600（`-wal`、`-shm` 不宽于 0600）。
- `secure_delete` 生效：写入一条带唯一合成标记的正文，删除之后，在数据库文件和 WAL 文件的原始字节里都找不到这个标记。

另外按任务要求：只读打开在库不存在时不建目录、不建库；库版本更新时报错且不改库。

验证只做这些：
- 上面每一条各有一个测试，全部只用临时目录和临时 `XDG_STATE_HOME` / `HOME`；再补和这些直接相关的边角用例（例如：已存在但权限过宽的目录怎么处理、迁移中途失败是否回滚），你判断需要就加，在完成记录里列出。
- `cargo test --all-targets` 和 `cargo clippy --all-targets -- -D warnings` 各跑一次并通过。
- 觉得不够，在回复里说，不要自己加。

## 不要做
- 不要读用户真实的 cairn 数据库，也不要在真实的 `~/.local/state` 下创建任何东西。测试一律用临时目录。
- 不要读真实会话记录、各工具的记忆文件。实现、测试、辅助脚本都不要用 Python。
- 不要动 `scope.rs`、`facts.rs`、`lib.rs`、`main.rs`、`tools/`、`docs/`（本任务文件的完成记录除外）。
- 依赖只用成熟、活跃维护的库（例如 `rusqlite` 带 `bundled`）。
- 遇到 DESIGN 的表结构说不通、需要改字段含义或删表的情况，停下来报告，等主控决定。
- 不要按项目名或路径批量杀进程（`pkill -f cairn` 这类）。
- 不合并到 main，不推送。只在 `p2a-store` 分支上提交。

## 做完
在本文件末尾追加「## 完成记录」（在你的分支里提交）：做了什么、验证了什么、拿主意的地方（尤其是表结构定稿时补的约束和索引）、没做的事，各几句话。回复里只写这几样，加上有没有要主控决定的事。命令都在前台跑完，全部做完后，回复最后一行写 DONE。

## 完成记录

2026-10-05，cairn/dev-store 完成。

- **做了什么**：实现 `store.rs` 与 `store/schema.rs`，使用 `rusqlite`（bundled）。`database_path(xdg_state_home, home)` 显式接收调用方取得的环境值，纯计算目标路径并词法规范化，不访问文件系统；空 XDG 值回退到 HOME，缺少可用根路径或相对根路径报错。`Store::open` 创建私有目录/文件、配置 SQLite 并迁移；`Store::open_read_only` 在库不存在时返回 `None`。提供 `connection()`、可选事务行为的 `transaction()`、幂等的 `migrate()` 和 `delete_body(id, deleted_at)`，时间戳由调用方提供。锁等待两档为 Hook 200 ms、UserCommand 2 s。
- **验证了什么**：新增 13 项存储集成测试。迁移幂等、纯路径计算、权限、未来版本无改写、缺库只读不创建、正文物理删除、只读拒绝宽权限，均先运行目标测试得到针对缺失行为的 RED，再实现并取得 GREEN；失败分别包括缺少 meta 表、路径未规范化、目录为 0755、未来库字节被 WAL 设置改变、只读路径创建了库、删除未执行、接受宽权限。正文测试先确认唯一合成标记同时存在于主库与 WAL，再验证删除后两处原始字节均不含标记，且墓碑元数据和其他正文保留。
- **直接相关的边角验证**：写入打开收紧已有目录/主库/WAL/SHM 权限；只读连接读取尚在 WAL 的已提交数据并拒绝写入；迁移创建部分表后发生冲突会回滚表与版本；版本仅在 WAL 更新为未来版本时，两种打开方式均拒绝且主库/WAL 字节不变；外键、枚举、操作唯一键及五种回合结果去重；删除遇到活动读事务时返回检查点繁忙，释放读事务后重试清除残留并保留首次删除时间。全部材料都在 `tempdir` 内，路径函数显式传入临时 XDG_STATE_HOME/HOME 值，不修改进程全局环境，不访问真实状态目录。
- **规定检查**：前台运行 `CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target cargo test --all-targets` 一次通过（13 项存储测试、4 项原有探针测试）；随后前台运行 `CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target cargo clippy --all-targets -- -D warnings` 一次通过。所有目标测试命令也均等待执行完毕。
- **表结构定稿**：保留 §6.2 全部十张表与字段含义。补文本主键 NOT NULL、adopted 的 0/1 约束，以及 association、records.kind、confirmations.kind、turn_decisions.outcome、spool_ops.outcome 的设计枚举约束；`events.kind` 按草案保持可扩展。保留原外键和唯一键，包含 `confirmations.op_id UNIQUE` 与 `spool_ops.op_id` 主键。新增五个索引：records 的项目/工作线/来源/时间/ID，以及 records.target_id、supersessions.target_id、confirmations 的来源/时间、events 的来源/时间。未增加业务触发器，调用方通过连接写 SQL 时仍须遵守记录只追加与连接设置约定。
- **其他拿主意的地方**：写入打开会收紧过宽权限，只读打开拒绝过宽权限而不 chmod；对已存在的 WAL 库，只读连接仍允许 SQLite 创建私有 sidecar，不迁移、不创建主库或目录。写入打开先用只读连接检查版本，避免先切 WAL 或关闭写连接时改写未来版本库。迁移用 IMMEDIATE 事务并在锁内复查版本。删除提交墓碑后检查 TRUNCATE 的返回状态，检查点繁忙不能当作成功；报错时墓碑可能已提交，调用方可安全重试。不存在的记录返回 false，重复删除保留最早 deleted_at。
- **没做的事**：未实现 save、暂存收取、渲染、回合判定、用户命令或安装；未修改 scope/facts/lib/main/tools、DESIGN 或 HANDOFF；未运行真实 agent、未改真实配置或访问真实 cairn 数据；不合并 main、不推送。
- **需要主控决定的事**：无。后续 2c–2f 可直接使用上述路径、连接与事务接口；2f 需把检查点失败作为删除尚未完成处理。

## 返工记录

2026-10-05，按主控转交的交叉审查第 1、2 条（必须改）和第 3 条（建议改）完成返工。主仓库的交叉审查文件仅读取，未修改。

- **修复第 1、2 条**：两种打开方式共用 `checked_database_path`，在任何 SQLite 打开或版本查询之前，用 `symlink_metadata`（lstat）核验库目录、主库、已存在 WAL/SHM：目录必须是真目录，文件必须是普通文件，属主必须为当前有效用户，权限不得宽于 0700/0600。符号链接（含悬空链接）直接报错；主库不存在时也先检查已存在的 sidecar。仅解析祖先路径的系统别名（例如 macOS 的 `/var`），并比对库目录的设备号/inode；不解析主库或 sidecar 链接。新主库以 `create_new`、0600、`O_NOFOLLOW` 创建，所有 SQLite 打开都带 `SQLITE_OPEN_NOFOLLOW`。新增直接依赖 libc，仅用于当前有效 uid 和 O_NOFOLLOW；没有变动现有依赖版本。
- **权限策略选择**：写入和只读打开都对已有宽权限路径直接拒绝，移除了存储层的 chmod。这取代上一节“写入打开自动收紧”的决定：检查版本前收紧权限会改动未来版本库，检查版本后再收紧又可能已生成宽权限 sidecar，因此无副作用拒绝最简单。只有主库已满足私有权限才进行版本预检，SQLite 新建 sidecar 时据此取得不宽于 0600 的权限。宽权限未来库在文件检查阶段报错，不查版本、不新建 sidecar、不改其内容或权限；权限合格的未来库仍通过读取实际 WAL 的只读预检返回 NewerSchema。
- **第 3 条**：在 `Store::open` 接口文档中写明，并发首次初始化可能立即返回 SQLITE_BUSY，busy_timeout 不保证等待满预算，调用方须按可重试错误处理。新增一次由屏障同步两个线程首次打开同一路径的测试：至少一个成功，其余只允许 DatabaseBusy；全部结束后重新打开，确认 integrity_check 为 ok、schema_version 为 1、十张表齐全。未增加内部重试，也未修改锁等待预算。
- **回归及 RED→GREEN**：仅新增指定的四项回归：主库链接（同一测试覆盖库目录链接）、sidecar 链接（WAL/SHM 与悬空链接）、已关闭且无 sidecar 的宽权限未来 WAL 库、并发首次初始化。前三项先在旧实现确认 RED，分别复现“只读接受主库链接”“WAL 链接目标被 chmod”“版本预检新建 WAL”，修复后定点测试全部 GREEN。链接回归检查拒绝后目标内容、权限和目录项不变；未来 WAL 回归检查主库字节/权限不变且未生成 sidecar。并发项是已有可重试行为的完整性回归，不伪造失败。
- **既有测试调整**：将“写入收紧权限”测试改为逐项验证拒绝宽权限且不 chmod，保持目录/主库/WAL/SHM 的覆盖。未来版本和迁移回滚测试的手工夹具改为 0700/0600，以便通过文件安全门后继续验证原有版本错误、字节不变和回滚断言；宽权限未来库由新增回归独立覆盖。其他业务断言不变。
- **规定验证**：所有命令在前台等待完成。`CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target cargo test --all-targets` 一次通过（17 项存储测试、4 项探针测试）；`CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target cargo clippy --all-targets -- -D warnings` 一次通过。此前仅运行上述新回归的定点 RED/GREEN；未扩展验证范围。所有材料均为隔离临时目录内的合成数据。
- **范围与交付**：仅修改存储实现、store 测试、依赖声明/锁文件及本返工记录；未改 schema、业务流程、主仓库审查文件或用户真实配置。未合并 main、未推送，无需主控决定的新事项，交主控复审。

## 主控审查

2026-10-05，cairn/main。结论：通过，已合并。
- 初审：只改了允许的文件；验收 3 条（迁移幂等、权限 0700/0600、删除后库和 WAL 里找不到原文）与"只读不建库、新版本库报错不改库"都有测试；重跑测试与 clippy 通过。
- 交叉审查（`docs/tasks/P2a-存储-交叉审查.md`）提出 2 条必须改：符号链接让权限检查落空；版本预检在收紧权限前生成宽权限 sidecar。返工（89453f5）改为打开 SQLite 之前统一核验、拒绝链接、宽权限一律拒绝不 chmod，并补并发首次打开的说明与测试；复核为"可以合并"。
- 主控同意"宽权限一律拒绝"的取舍：cairn 自建的目录和文件本来就是私有的，拒绝没有副作用，也不会改动未来版本的库。
- 合并时解决了与 2b 在 `crates/cairn/Cargo.toml`、`Cargo.lock` 上的依赖冲突（取两边依赖的并集），main 上 `cargo test --all-targets`（作用域 5、存储 17、探针 4）和 clippy 通过。
- 给 2f 的提醒：`delete_body` 返回 `CheckpointBusy` 时墓碑已提交、物理清除未完成，不能报告删除成功。
