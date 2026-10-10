# 交叉审查：F2 hook 最近触发时间（表结构版本 2）与公开约定

2026-10-10，paddock/main 交给 cairn/review-f2（Codex，重：gpt-6-astra / xhigh）。
类型：其他（只读审查）
你是被委派的 agent：照本文件做，不要再开别的 agent。

## 背景
paddock/main 照用户的话直接在 cairn 仓库做了 F2（任务书：`docs/tasks/F2-hook最近触发与公开约定.md`，在分支里）：分支 `f2-hook-seen`，一个提交 `f807bff`。做了三件事：表结构升到版本 2，加 `hook_seen` 表，四种 hook 事件处理时各记一行最近时间；`cairn status --json` 加 `agents.<名字>.last_seen`；DESIGN 加 §6.2 版本 2、§7.1 公开约定、§8.7。这是 cairn 第一次改表结构、第一次升级已有的数据库，用户机器上有一个版本 1 的真实数据库（两个试点项目的接续记录只在里面），装上新版后第一次读写打开就会被升级，所以请你审。作者自己跑过 `cargo test --all-targets`（110 项）和 `cargo clippy --all-targets -- -D warnings`，都通过。

## 先读
- `AGENTS.md`（规矩一节）
- `docs/tasks/F2-hook最近触发与公开约定.md`
- `docs/DESIGN.md` §6.1、§6.2（含末尾“版本 2”）、§7.1、§8.6、§8.7

## 要审查的代码
- 你在自己的 worktree 里：`/Users/firegnu/Developer/personal_projs/cairn-worktrees/review-f2-hook-seen`（detached，就是 `f807bff`）。看 `git diff main...f807bff`。
- **只读：不改、不提交、不切分支。** 唯一可以写的是本文件（它在主仓库 `/Users/firegnu/Developer/personal_projs/cairn/docs/tasks/` 下，不在你的 worktree 里），只往末尾追加。
- 可以跑测试：命令前加 `CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target-review-f2`（用你自己的编译目录）。可以跑 `cargo test --all-targets`、单个测试，或者在临时目录里用合成的版本 1 数据库手试（`crates/cairn/tests/fixtures/schema_v1.sql` 是冻结的版本 1 表结构）。
- 不读真实的会话记录、真实的 cairn 数据库（`~/.local/state/cairn/`）、`~/.claude*`、`~/.codex`；不在真实 HOME 下运行 `cairn install`、`adopt` 等会写状态的命令。不运行真实 agent。不用 Python。不要按名字批量杀进程。
- 验证照“碰要害”的预算：读代码为主，可疑处挑一两条实测；不做覆盖矩阵、不录屏。

## 重点看
1. **升级会不会损坏或丢掉版本 1 库里的数据**（`store.rs` 的 `migrate`、`open`、`open_read_only`，`store/schema.rs` 的 `V2`）：中途失败（磁盘满、被杀、SQLITE_BUSY）后库是不是还是完好的版本 1 或完好的版本 2；`meta.schema_version` 和表是不是一起变；WAL 模式下有没有问题。
2. **并发**：两个 hook（或一个 hook 和一个用户命令）同时对版本 1 的库做读写打开，会不会重复建表、报出 BUSY 以外的错、或者留下半升级的库；只读探测和随后的读写打开之间别人升了级，有没有问题。
3. **只读打开放宽到“1 到当前版本都接受”之后**，所有只读路径（`status.rs`、`session.rs` 和 `turn.rs` 里的探测、`cli.rs` 的 `existing_store`）在版本 1 的库上会不会碰到不存在的 `hook_seen`；比当前版本新的库是不是照旧被拒绝且不被改动。
4. **`hook_seen` 的写入**（`turn.rs` 的 `hook_seen`、`seen`，`session.rs`）：有没有改变原来的 hook 行为——特别是 `turn_ended` 里“收取出错且本该续跑”的提前返回，原来事务回滚，现在提交；这之前事务里是不是确实只有这一条写入。写入失败会不会让原来能成功的 hook 变成失败。未采用、`CAIRN_DISABLE=1` 时确实不写。
5. **DESIGN §7.1 写的约定和代码实际输出是否一致**（`status --json` 的四组字段、`show --json` 的两种形状、`adopt` 的退出码和输出）。约定一旦写下就不能改，写错了要现在指出。

## 输出
追加到本文件末尾「## 审查意见」。先写一句结论（可以合并 / 改完再合并）。每条意见写：级别（必须改 / 建议改 / 可以不改）、位置、问题、改法。**「必须改」只给两种**：任务书「怎么算做完」没达到；上面这些要害上有真实缺陷（会丢数据或损坏库、并发出错、泄露、约定和实际不符）。其余一律「建议改」。

## 回复
只写结论和各级别的条数。命令都在前台跑完，全部做完后，回复最后一行写 DONE。

## 审查意见

结论：**改完再合并**。必须改 2 条，建议改 1 条，可以不改 0 条。

审查对象：`f807bff8fdcaa3f2aa98e04bb49d87b48dbb8816`（相对 `main` 的基点 `08ce9798e5c7d9e8149faf3a7b2f3adddabeb52f`），2026-10-10。

### R1 — 必须改：`show --json` 的未采用项目约定与实际输出不符

- **位置**：`docs/DESIGN.md:307`；对应实现 `crates/cairn/src/cli.rs:192–220`、`crates/cairn/src/render.rs:65–82,164–182`。
- **问题**：新公开约定写“项目没采用时只有抬头”，但 `show` 不按 `projects.adopted` 过滤，取消采用保留的旧记录仍会展示。这里不是要求修改旧的查看行为，而是本次新增的“只加不改”约定写错了。
- **证据**：隔离 HOME / 状态目录 / 暂存目录下，采用合成项目，插入一条正文含 `SYNTHETIC_OLD_BODY` 的合成 checkpoint，再通过 CLI 分发调用 `unadopt`。之后 `status --json` 为 `project.status="not_adopted"`，`show --json` 的 `text` 仍含该正文，`record_ids=["synthetic-record"]`。同一夹具也确认无数据库时输出 `{"status":"no_data"}`、重复采用成功、采用及取消采用的返回文本符合 §7.1。
- **改法**：按现有行为修正文档：没有项目记录时只有抬头；取消采用不阻止用户命令查看历史记录。合并前把这条公开约定与上述输出对齐，并保留一个小型契约回归检查。不要为了配合错误的文档而顺手改变既有历史查看行为。

### R2 — 必须改：SessionStart 在并发取消采用后仍写入 `hook_seen`

- **位置**：`crates/cairn/src/session.rs:42–55,74`，`crates/cairn/src/turn.rs:343–347`；对应 `docs/DESIGN.md:400–405` 的采用门槛。
- **问题**：SessionStart 只在只读探测阶段检查采用状态；开始写事务后没有重查，而新增的 `hook_seen` SQL 只检查项目键、不检查 `adopted=1`。若用户的取消采用事务在探测后、hook 写事务前提交，hook 仍会新增或刷新 SessionStart 时间，违反“取消采用后保留已有行、重新采用后接着更新”的约定。其他三个事件在写事务内都重查了采用状态。SessionStart 原有的检查空隙早已存在；F2 的新增调用把这个问题带入了本次 `last_seen` 功能。
- **证据**：合成 WAL 数据库中项目初始 `adopted=1`、没有任何 `hook_seen`。连接 A 持有 `BEGIN IMMEDIATE` 并执行尚未提交的取消采用；线程 B 运行真实 `session::start`，只读探测仍看到已采用。以 B 随后创建隔离暂存命名空间作为“已通过探测”的同步点，A 提交，再等待 B 完成。结果为 `adopted=0, hook_seen rows=1, injected=true`。该顺序不依靠猜测 hook 执行到哪一步，也未改生产代码或超时。
- **改法**：与 `turn_started` / `session_ended` 一致，在 SessionStart 取得 `IMMEDIATE` 事务后、写来源和事件之前再次查询当前项目是否采用；已取消则直接返回 `None`，不写本次时间、不注入。加一个同步到探测之后再提交取消采用的直接回归检查。

### S1 — 建议改：补一个真正覆盖 v1→v2 中途失败的回滚检查

- **位置**：`crates/cairn/tests/store.rs:370–394,707–788`。
- **问题**：现有 `migration_failure_rolls_back_tables_and_version` 验证的是未版本化数据库执行 V1 期间失败；新增 v1→v2 检查只有成功路径，而且旧数据只放了项目和事件。它们没有直接验证本次最关键的“V2 已建表，但写版本号失败”的情况。代码中的事务结构目前看正确，未发现迁移丢数据或半升级缺陷，因此此项为建议改。
- **改法**：用冻结的 v1 结构及一条带正文的合成旧记录，在 `meta` 更新处注入一次失败；断言失败后仍为版本 1、没有 `hook_seen`、旧正文和关联保持不变，移除故障后再次打开能升级。一个针对性的检查即可，不需要故障覆盖矩阵。

### 核对与验证边界

- 已核对迁移、所有只读打开的调用点、四种事件写入、Stop 收取失败分支及 §7.1 的三条命令。`migrate` 在 `IMMEDIATE` 事务内重新读版本，V2 建表和版本更新一起提交；V2 仅加表，没有修改旧业务表或重排 rowid。只读 v1 的 `status` 在查 `hook_seen` 前检查表是否存在，其余只读探测只读旧表；随后读写打开会重新检查版本。未发现这些路径的半升级或重复建表问题。
- Stop 的特殊提前返回之前，在该判定事务里确实只有 `hook_seen` 一项写入；收取在此前的独立事务中完成。新增写入失败会使所属事务回滚并经既有 hook 边界放行、记录错误，符合 §8.6 的失败策略。禁用和串行未采用路径由已有测试验证；并发取消采用的 SessionStart 缺口见 R2。
- 前台跑完：`CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target-review-f2 cargo test --test store --test hook --test turn`，**48 项通过**（store 20、hook 12、turn 16）。其中包含 v1 只读/升级、并发升级、WAL 中较新版本拒绝且字节不变、四事件时间、禁用/取消采用和 Stop 收取失败回归。
- 另用链接当前构建产物的临时 Rust 夹具完成上述两项实测，均复现问题；全部使用合成内容和显式隔离路径，临时夹具已退出并清理。仓库源码、测试和分支未改动。本轮按审查预算未重复作者的全量测试或 clippy，也未做磁盘满/杀进程实测；现有并发升级测试使用两个线程、独立 SQLite 连接，并非任务书所写的两个进程。

## 复核意见

结论：**改完再合并**。本轮必须改 1 条，建议改 0 条，可以不改 0 条；仅剩 R1 修订文字中的一处公开约定不符，R2、S1 已通过复核。

2026-10-10，复核 `f807bff..f59f10e`，当前 detached HEAD 为 `f59f10ef131ee32f55b1078b6f31c1d97ab355a9`。范围仅限上一轮意见的修复及本次改动的直接回归。

### R1 补充 — 必须改：新增的“没有可显示的记录时只有抬头”仍不准确

- **位置**：本次修改的 `docs/DESIGN.md:307`；对照 `crates/cairn/src/render.rs:164–182,350–370`。
- **问题**：原来“未采用项目只显示抬头”的问题已修正，新增测试也确认取消采用后仍能查看历史。不过替换后的“这个项目没有可显示的记录时只有抬头”扩大了条件：项目曾有记录、之后全部删除时，已没有可显示的正文，`record_ids` 也为空，但 `text` 仍在抬头后附加“未显示的来源：0；查看 `cairn list --line`。”。实现只有在项目数据库记录集合完全为空时才直接返回抬头，删除留下的墓碑不满足这个条件。这是本次新增文字与输出的直接不符，不是要求修改既有渲染行为。
- **证据**：临时 Rust 夹具链接 `f59f10e` 构建产物，用显式隔离的 HOME、状态和暂存路径：采用项目，保存空项目的 `show --json` 文本作抬头基线；插入一条合成 checkpoint，再经 CLI 分发调用 `delete synthetic-record --yes` 和 `show --json`。实测 `record_ids=[]`、合成正文已不在输出中，但相对抬头多出 `\n未显示的来源：0；查看 `cairn list --line`。\n`（此处 `\n` 表示换行）。
- **改法**：只修正文档即可：将条件收窄为“这个项目在数据库中没有任何记录时只有抬头”，或删去这项对空内容文本形状的保证；保留“取消采用后历史仍可查看”的说明。无需改变产品代码或扩大测试范围。

### 已通过的修复与验证

- **R1 原问题已修复**：§7.1 明确取消采用不隐藏历史；`show_json_keeps_its_two_shapes_and_still_shows_records_once_unadopted` 验证无库形状、空项目形状，以及 `not_adopted` 下仍返回旧正文和记录 ID。上面的补充只针对这次替换句子新引入的过宽条件。
- **R2 已修复**：`session.rs:55–64` 在取得 `IMMEDIATE` 事务后、来源/事件/`hook_seen`/注入写入前重查采用状态；已取消时返回 `None`。新增并发测试以探测之后创建暂存命名空间为同步点，提交取消采用后断言不注入，且 `hook_seen`、`sources`、`events`、`injections` 均无新行。修复位置和测试都覆盖上一轮复现顺序，正常 SessionStart、resume/fork、禁用及未采用的现有直接回归也通过。
- **S1 已落实**：`failed_upgrade_leaves_a_version_1_database_as_it_was_and_a_later_open_upgrades_it` 在更新版本号时用触发器注入失败，检查版本仍为 1、表结构不变、旧正文及来源/项目关联不变、只读打开成功；移除故障后能升到 2 且只有一张 `hook_seen`。
- **前台验证**：`CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target-review-f2 cargo test --test render_session --test store`，**35 项通过**（14 + 21）；`git diff --check f807bff..f59f10e` 通过。另完成上述一个合成输出核验，临时程序已退出并清理。本轮未重复全量测试或 clippy。
- 工作区源码、测试和分支未修改；本轮仅向本审查文件追加本节。未发现本次 SessionStart 修复引入新的运行时问题，也未新增与本次改动无关的审查项。

## 第二次复核

结论：**可以合并**。必须改 0 条，建议改 0 条，可以不改 0 条。

2026-10-10，仅复核 `f59f10e..0d93c3c` 的 `docs/DESIGN.md:307`，当前 detached HEAD 为 `0d93c3cd3903a40db126d37a38be16a5c8599e35`。

R1 补充已修好：“只有抬头”的条件已收窄为“这个项目在数据库里没有任何记录”，并明确曾有记录但已全部删除或撤回时仍可能附带其他文字，保留取消采用后历史照样显示的说明。该修订与上一轮核对的实现及合成输出一致，上一轮剩余必须改项关闭；R2、S1 沿用已通过的复核结论。

已确认该提交仅修改上述一行文档，`git diff --check f59f10e..0d93c3c` 通过。纯文档修订未重跑测试，未扩大审查范围；本轮仅向本文件追加本节，工作区源码、测试和分支未修改。

## 主控结论

2026-10-10，paddock/main。三轮：第一轮 R1、R2 必须改、S1 建议改，都采纳（提交 `f59f10e`）；复核又指出 R1 改后的句子条件写宽了，收窄（`0d93c3c`）；第二次复核“可以合并”。没有不采纳的意见。审查员没重跑全量测试和 clippy，作者在 `f59f10e` 上跑过（113 项通过，clippy 干净），之后只改了文档一行。已合并进 main。
