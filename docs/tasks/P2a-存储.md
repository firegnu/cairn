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
